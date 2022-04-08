use std::collections::VecDeque;

use ndarray::{Array2, Array3};
use pathfinding::prelude::astar;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::utils::{position_to_cell, Error, Vec3, AABB};

const EXCLUDE_OBJECT_IDS: [u32; 12] = [4, 13, 14, 15, 18, 23, 26, 29, 32, 38, 45, 77];
const MAX_MAP_BOUNDS: AABB = AABB {
    min_x: -800.0,
    min_y: -200.0,
    min_z: -800.0,
    max_x: 800.0,
    max_y: 200.0,
    max_z: 800.0,
};

pub(crate) const CELL_SIZE: f32 = 2.5;
const CHUNK_SIZE: f32 = 130.0 * CELL_SIZE;
const PLAYER_HEIGHT: usize = (15.0 / CELL_SIZE) as usize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RawMapObject {
    #[serde(rename = "p")]
    pub position: [f32; 3],
    #[serde(rename = "si")]
    pub size_index: Option<usize>,
    #[serde(rename = "i")]
    pub id: Option<u32>,
    #[serde(rename = "l")]
    pub not_collidable: Option<u8>,
    #[serde(rename = "bo")]
    pub border: Option<u8>,
    #[serde(rename = "d")]
    pub direction: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMapConfig {
    pub modes: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMap {
    pub name: String,
    #[serde(rename = "xyz")]
    pub sizes: Vec<f32>,
    pub objects: Vec<RawMapObject>,
    pub config: RawMapConfig,
    pub spawns: Vec<Vec<Option<f32>>>,
}

impl RawMap {
    fn get_size_groups(&self) -> Vec<Vec3> {
        let mut res = Vec::<Vec3>::new();

        for i in (0..).step_by(3) {
            let x = if let Some(x) = self.sizes.get(i) {
                *x
            } else {
                break;
            };
            let y = if let Some(y) = self.sizes.get(i + 1) {
                *y
            } else {
                break;
            };
            let z = if let Some(z) = self.sizes.get(i + 2) {
                *z
            } else {
                break;
            };

            res.push(Vec3 { x, y, z });
        }

        res
    }
}

#[derive(Debug, Clone, Copy)]
struct Ramp {
    bounds: AABB,
    direction: u8,
}

type FilteredObjects = (AABB, Vec<AABB>, Vec<Ramp>);

#[derive(Debug, Clone)]
struct Chunk<'a> {
    bounds: AABB,
    objects: Vec<&'a AABB>,
    ramps: Vec<&'a Ramp>,
}

#[derive(Debug, Clone)]
pub struct Map {
    pub(crate) name: String,
    pub(crate) spawns: Vec<Vec3>,
    pub(crate) bounds: AABB,
    pub(crate) walkable_grid: Array3<u8>,
}

impl Map {
    pub fn new(raw_map: &RawMap) -> Result<Self, Error> {
        debug!("Loading {}", raw_map.name);

        let (map_bounds, objects, ramps) = Self::filter_objects(raw_map)?;

        let spawns = raw_map
            .spawns
            .iter()
            .map(|s| {
                if s.len() < 3 {
                    Err("Raw map spawn contains less than 3 coordinates".into())
                } else {
                    Ok(Vec3 {
                        x: s[0].ok_or("Spawn coordinate is null")?,
                        y: s[1].ok_or("Spawn coordinate is null")?,
                        z: s[2].ok_or("Spawn coordinate is null")?,
                    })
                }
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let walkable_grid = Self::generate_walkable_grid(
            &Self::generate_grid(
                &map_bounds,
                &Self::generate_object_chunks(&map_bounds, &objects, &ramps),
            ),
            &map_bounds,
            &spawns,
        )?;

        debug!("Finished loading {}", raw_map.name);

        Ok(Self {
            name: raw_map.name.clone(),
            spawns,
            bounds: map_bounds,
            walkable_grid,
        })
    }

    fn filter_objects(raw: &RawMap) -> Result<FilteredObjects, Error> {
        let mut map_bounds = AABB::zero();

        // estimate the number of objects to avoid frequent allocation
        let mut objects = Vec::<AABB>::with_capacity(raw.objects.len() / 3);
        let mut ramps = Vec::<Ramp>::new();

        let sizes = raw.get_size_groups();
        for object in raw.objects.iter() {
            // filter out everything that is not collidable
            if object.not_collidable.is_some() {
                continue;
            }

            if let Some(id) = object.id {
                if EXCLUDE_OBJECT_IDS.contains(&id) {
                    continue;
                }
            }

            if let Some(size_index) = object.size_index {
                let size = sizes
                    .get(size_index)
                    .ok_or("Raw map object size index out of bounds")?;

                let mut bounds = AABB {
                    min_x: object.position[0] - size.x / 2.0,
                    min_y: object.position[1],
                    min_z: object.position[2] - size.z / 2.0,
                    max_x: object.position[0] + size.x / 2.0,
                    max_y: object.position[1] + size.y,
                    max_z: object.position[2] + size.z / 2.0,
                };

                map_bounds.extend_by(&bounds);

                // extend the height of the object if it is a border object
                if object.border.is_some() {
                    bounds.max_y = MAX_MAP_BOUNDS.max_y;
                }

                if let Some(id) = object.id {
                    if id == 9 {
                        ramps.push(Ramp {
                            bounds,
                            direction: object.direction.unwrap_or(0),
                        });
                        continue;
                    }
                }

                objects.push(bounds);
            }
        }

        map_bounds.limit_by(&MAX_MAP_BOUNDS);

        Ok((map_bounds, objects, ramps))
    }

    fn generate_object_chunks<'a>(
        map_bounds: &AABB,
        objects: &'a [AABB],
        ramps: &'a [Ramp],
    ) -> Array2<Chunk<'a>> {
        let chunk_shape = (
            ((map_bounds.max_x - map_bounds.min_x) / CHUNK_SIZE).ceil() as usize,
            ((map_bounds.max_z - map_bounds.min_z) / CHUNK_SIZE).ceil() as usize,
        );

        Array2::<Chunk<'a>>::from_shape_fn(chunk_shape, |(x, z)| {
            let chunk_bounds = AABB {
                min_x: map_bounds.min_x + x as f32 * CHUNK_SIZE,
                min_y: MAX_MAP_BOUNDS.min_y,
                min_z: map_bounds.min_z + z as f32 * CHUNK_SIZE,
                max_x: map_bounds.min_x + x as f32 * CHUNK_SIZE + CHUNK_SIZE,
                max_y: MAX_MAP_BOUNDS.max_y,
                max_z: map_bounds.min_z + z as f32 * CHUNK_SIZE + CHUNK_SIZE,
            };

            let mut chunk_objects =
                Vec::<&'a AABB>::with_capacity(objects.len() / (chunk_shape.0 * chunk_shape.1));
            let mut chunk_ramps = Vec::<&'a Ramp>::new();

            for object in objects.iter() {
                if chunk_bounds.intersects(object) {
                    chunk_objects.push(object);
                }
            }

            for ramp in ramps.iter() {
                if chunk_bounds.intersects(&ramp.bounds) {
                    chunk_ramps.push(ramp);
                }
            }

            Chunk {
                bounds: chunk_bounds,
                objects: chunk_objects,
                ramps: chunk_ramps,
            }
        })
    }

    fn generate_grid<'a>(map_bounds: &AABB, chunks: &Array2<Chunk<'a>>) -> Array3<u8> {
        let grid_shape = (
            ((map_bounds.max_x - map_bounds.min_x) / CELL_SIZE).ceil() as usize,
            ((map_bounds.max_y - map_bounds.min_y) / CELL_SIZE).ceil() as usize,
            ((map_bounds.max_z - map_bounds.min_z) / CELL_SIZE).ceil() as usize,
        );

        Array3::<u8>::from_shape_fn(grid_shape, |(x, y, z)| {
            let cell_bounds = AABB {
                min_x: map_bounds.min_x + x as f32 * CELL_SIZE,
                min_y: map_bounds.min_y + y as f32 * CELL_SIZE,
                min_z: map_bounds.min_z + z as f32 * CELL_SIZE,
                max_x: map_bounds.min_x + x as f32 * CELL_SIZE + CELL_SIZE,
                max_y: map_bounds.min_y + y as f32 * CELL_SIZE + CELL_SIZE,
                max_z: map_bounds.min_z + z as f32 * CELL_SIZE + CELL_SIZE,
            };

            for chunk in chunks.iter() {
                if chunk.bounds.intersects(&cell_bounds) {
                    let mut cell = 0_u8;

                    for ramp in chunk.ramps.iter() {
                        if cell_bounds.intersects(&ramp.bounds) {
                            cell = 2 + ramp.direction;
                            break;
                        }
                    }

                    for object in chunk.objects.iter() {
                        if cell_bounds.intersects(object) {
                            cell = 1;
                            break;
                        }
                    }

                    return cell;
                }
            }

            panic!("Cell not in a chunk");
        })
    }

    fn generate_walkable_grid(
        grid: &Array3<u8>,
        map_bounds: &AABB,
        spawns: &[Vec3],
    ) -> Result<Array3<u8>, Error> {
        let shape = grid.shape();
        let grid_size = (shape[0], shape[1], shape[2]);

        let mut walkable_grid = Array3::<u8>::zeros(grid_size);

        // start with all spawn cells as we expect the player to be able to stand there
        let mut cells_to_see = VecDeque::from(
            spawns
                .iter()
                .map(|spawn| {
                    let mut cell = position_to_cell(map_bounds, spawn);
                    if grid[cell] != 0 {
                        cell.1 += 1;
                    }
                    cell
                })
                .collect::<Vec<_>>(),
        );

        while let Some(cell) = cells_to_see.pop_front() {
            if cell.0 >= grid_size.0 || cell.1 >= grid_size.1 || cell.2 >= grid_size.2 {
                return Err("Cell index out of bounds".into());
            }

            if walkable_grid[cell] != 0 {
                continue;
            }

            walkable_grid[cell] = 1;

            if grid[cell] == 0 {
                for neighbour in Self::horizontal_neighbours(&cell, &grid_size, false).iter() {
                    if Self::is_cell_walkable(neighbour, grid) {
                        cells_to_see.push_back(*neighbour);
                    } else if Self::is_cell_walkable(
                        &(neighbour.0, neighbour.1 + 1, neighbour.2),
                        grid,
                    ) {
                        cells_to_see.push_back((neighbour.0, neighbour.1 + 1, neighbour.2));
                    } else if Self::is_cell_walkable(
                        &(neighbour.0, neighbour.1 - 1, neighbour.2),
                        grid,
                    ) {
                        cells_to_see.push_back((neighbour.0, neighbour.1 - 1, neighbour.2));
                    }
                }
            } else {
                for neighbour in Self::neighbours(&cell, &grid_size, true).iter() {
                    if Self::is_cell_walkable(neighbour, grid) {
                        cells_to_see.push_back(*neighbour);
                    }
                }
            }
        }

        Ok(walkable_grid)
    }

    fn neighbours(
        cell: &(usize, usize, usize),
        grid_size: &(usize, usize, usize),
        edges: bool,
    ) -> Vec<(usize, usize, usize)> {
        let cell = (cell.0 as isize, cell.1 as isize, cell.2 as isize);

        let mut neighbours = vec![
            (cell.0, cell.1 + 1, cell.2),
            (cell.0 - 1, cell.1 + 1, cell.2),
            (cell.0 + 1, cell.1 + 1, cell.2),
            (cell.0, cell.1 + 1, cell.2 - 1),
            (cell.0, cell.1 + 1, cell.2 + 1),
            (cell.0 - 1, cell.1, cell.2),
            (cell.0 + 1, cell.1, cell.2),
            (cell.0, cell.1, cell.2 - 1),
            (cell.0, cell.1, cell.2 + 1),
            (cell.0, cell.1 - 1, cell.2),
            (cell.0 - 1, cell.1 - 1, cell.2),
            (cell.0 + 1, cell.1 - 1, cell.2),
            (cell.0, cell.1 - 1, cell.2 - 1),
            (cell.0, cell.1 - 1, cell.2 + 1),
        ];

        if edges {
            neighbours.append(&mut vec![
                (cell.0 - 1, cell.1 + 1, cell.2 - 1),
                (cell.0 - 1, cell.1 + 1, cell.2 + 1),
                (cell.0 + 1, cell.1 + 1, cell.2 - 1),
                (cell.0 + 1, cell.1 + 1, cell.2 + 1),
                (cell.0 - 1, cell.1, cell.2 - 1),
                (cell.0 - 1, cell.1, cell.2 + 1),
                (cell.0 + 1, cell.1, cell.2 - 1),
                (cell.0 + 1, cell.1, cell.2 + 1),
                (cell.0 - 1, cell.1 - 1, cell.2 - 1),
                (cell.0 - 1, cell.1 - 1, cell.2 + 1),
                (cell.0 + 1, cell.1 - 1, cell.2 - 1),
                (cell.0 + 1, cell.1 - 1, cell.2 + 1),
            ]);
        }

        neighbours
            .iter()
            .filter_map(|(x, y, z)| {
                if (*x >= 0 && *x < grid_size.0 as isize)
                    && (*y >= 0 && *y < grid_size.1 as isize)
                    && (*z >= 0 && *z < grid_size.2 as isize)
                {
                    Some((*x as usize, *y as usize, *z as usize))
                } else {
                    None
                }
            })
            .collect()
    }

    fn horizontal_neighbours(
        cell: &(usize, usize, usize),
        grid_size: &(usize, usize, usize),
        edges: bool,
    ) -> Vec<(usize, usize, usize)> {
        let cell = (cell.0 as isize, cell.1, cell.2 as isize);

        let mut neighbours = vec![
            (cell.0 - 1, cell.1, cell.2),
            (cell.0 + 1, cell.1, cell.2),
            (cell.0, cell.1, cell.2 - 1),
            (cell.0, cell.1, cell.2 + 1),
        ];

        if edges {
            neighbours.append(&mut vec![
                (cell.0 - 1, cell.1, cell.2 - 1),
                (cell.0 - 1, cell.1, cell.2 + 1),
                (cell.0 + 1, cell.1, cell.2 - 1),
                (cell.0 + 1, cell.1, cell.2 + 1),
            ]);
        }

        neighbours
            .iter()
            .filter_map(|(x, y, z)| {
                if *x >= 0 && *x < grid_size.0 as isize && *z >= 0 && *z < grid_size.2 as isize {
                    Some((*x as usize, *y, *z as usize))
                } else {
                    None
                }
            })
            .collect()
    }

    fn is_cell_walkable(cell: &(usize, usize, usize), grid: &Array3<u8>) -> bool {
        let shape = grid.shape();
        let grid_size = (shape[0], shape[1], shape[2]);

        // check if the following checks are in bounds
        if cell.0 + 1 >= grid_size.0
            || cell.1 + PLAYER_HEIGHT > grid_size.1
            || cell.2 + 1 >= grid_size.2
        {
            return false;
        }

        // check that cell and cells above are air or ramp
        for i in 0..(PLAYER_HEIGHT - 1) {
            if grid[(cell.0, cell.1 + i, cell.2)] == 1 {
                return false;
            }
        }

        // check that cell below is filled or ramp
        if grid[(cell.0, cell.1 - 1, cell.2)] == 0 {
            return false;
        }

        if grid[*cell] == 0 {
            // make sure that all filled blocks connected to the wrong site of a ramp are not walkable
            for i in 0..2 {
                if grid[(cell.0 - 1, cell.1 - i, cell.2)] == 3
                    || grid[(cell.0 - 1, cell.1 - i, cell.2)] == 5
                    || grid[(cell.0 + 1, cell.1 - i, cell.2)] == 3
                    || grid[(cell.0 + 1, cell.1 - i, cell.2)] == 5
                    || grid[(cell.0, cell.1 - i, cell.2 - 1)] == 2
                    || grid[(cell.0, cell.1 - i, cell.2 - 1)] == 4
                    || grid[(cell.0, cell.1 - i, cell.2 + 1)] == 2
                    || grid[(cell.0, cell.1 - i, cell.2 + 1)] == 4
                {
                    return false;
                }
            }

            for neighbour in Self::horizontal_neighbours(cell, &grid_size, true) {
                // check that surrounding cells are filled or ramp on the same height or one above or below
                if grid[(neighbour.0, neighbour.1 - 2, neighbour.2)] == 0
                    && grid[(neighbour.0, neighbour.1 - 1, neighbour.2)] == 0
                    && grid[neighbour] == 0
                {
                    return false;
                }

                // check that surrounding cells above are air or ramp
                if grid[(neighbour.0, neighbour.1 + 1, neighbour.2)] == 1 {
                    return false;
                }
            }
        }

        true
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn spawns(&self) -> Vec<Vec3> {
        self.spawns.clone()
    }

    pub fn closest_walkable_cell(&self, position: &Vec3) -> Option<(usize, usize, usize)> {
        if !self.bounds.contains(position) {
            return None;
        }

        let shape = self.walkable_grid.shape();
        let grid_size = (shape[0], shape[1], shape[2]);

        let exact_cell = position_to_cell(&self.bounds, position);
        let mut cells = Vec::<(usize, usize, usize)>::new();

        for y in 0..grid_size.1 {
            if self.walkable_grid[(exact_cell.0, y, exact_cell.2)] != 0 {
                cells.push((exact_cell.0, y, exact_cell.2));
            }
        }

        cells.sort_by(|a, b| {
            (a.1 as isize - exact_cell.1 as isize)
                .abs()
                .cmp(&(b.1 as isize - exact_cell.1 as isize).abs())
        });

        cells.get(0).cloned()
    }

    pub fn find_path(
        &self,
        start_cell: &(usize, usize, usize),
        end_cell: &(usize, usize, usize),
    ) -> Option<Vec<(usize, usize, usize)>> {
        let shape = self.walkable_grid.shape();
        let grid_size = (shape[0], shape[1], shape[2]);

        let path = astar(
            start_cell,
            |cell| {
                Self::neighbours(cell, &grid_size, false)
                    .iter()
                    .filter_map(|c| {
                        if self.walkable_grid[*c] > 0 {
                            for n in Self::horizontal_neighbours(c, &grid_size, true) {
                                if self.walkable_grid[n] == 0
                                    && self.walkable_grid[(n.0, n.1 + 1, n.2)] == 0
                                    && self.walkable_grid[(n.0, n.1 - 1, n.2)] == 0
                                {
                                    return Some((*c, 3));
                                }
                            }

                            Some((*c, if cell.1 == c.1 { 1 } else { 2 }))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            },
            |cell| {
                ((cell.0 as f32 - end_cell.0 as f32).powi(2)
                    + (cell.1 as f32 - end_cell.1 as f32).powi(2)
                    + (cell.2 as f32 - end_cell.2 as f32).powi(2))
                .sqrt()
                .floor() as u32
            },
            |cell| *cell == *end_cell,
        );

        if let Some((path, _)) = path {
            Some(self.simplify_path(&path))
        } else {
            None
        }
    }

    fn simplify_path(&self, path: &[(usize, usize, usize)]) -> Vec<(usize, usize, usize)> {
        if path.len() <= 2 {
            return Vec::from(path);
        }

        let mut simplified_path = Vec::from([path[0]]);
        let mut from_cell = path[0];
        let mut last_cell = path[1];
        'outer: for cell in &path[2..] {
            for x in cell.0.min(from_cell.0)..cell.0.max(from_cell.0) + 1 {
                for z in cell.2.min(from_cell.2)..cell.2.max(from_cell.2) + 1 {
                    let mut found_filled = false;
                    for y in cell.1.min(from_cell.1)..cell.1.max(from_cell.1) + 1 {
                        if self.walkable_grid[(x, y, z)] > 0 {
                            found_filled = true;
                            break;
                        }
                    }

                    if !found_filled {
                        simplified_path.push(last_cell);
                        from_cell = last_cell;
                        last_cell = *cell;
                        continue 'outer;
                    }
                }
            }

            last_cell = *cell;
        }

        simplified_path.push(last_cell);
        simplified_path
    }
}
