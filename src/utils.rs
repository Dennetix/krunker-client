use crate::map::CELL_SIZE;

pub type Error = Box<dyn std::error::Error + Sync + Send>;

#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
}

impl AABB {
    pub fn zero() -> Self {
        AABB {
            min_x: 0.0,
            min_y: 0.0,
            min_z: 0.0,
            max_x: 0.0,
            max_y: 0.0,
            max_z: 0.0,
        }
    }

    pub fn extend_by(&mut self, other: &Self) {
        if other.min_x < self.min_x {
            self.min_x = other.min_x;
        }
        if other.min_y < self.min_y {
            self.min_y = other.min_y;
        }
        if other.min_z < self.min_z {
            self.min_z = other.min_z;
        }
        if other.max_x > self.max_x {
            self.max_x = other.max_x;
        }
        if other.max_y > self.max_y {
            self.max_y = other.max_y;
        }
        if other.max_z > self.max_z {
            self.max_z = other.max_z;
        }
    }

    pub fn limit_by(&mut self, other: &Self) {
        if self.min_x < other.min_x {
            self.min_x = other.min_x;
        }
        if self.min_y < other.min_y {
            self.min_y = other.min_y;
        }
        if self.min_z < other.min_z {
            self.min_z = other.min_z;
        }
        if self.max_x > other.max_x {
            self.max_x = other.max_x;
        }
        if self.max_y > other.max_y {
            self.max_y = other.max_y;
        }
        if self.max_z > other.max_z {
            self.max_z = other.max_z;
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        (self.min_x < other.max_x && self.max_x > other.min_x)
            && (self.min_y < other.max_y && self.max_y > other.min_y)
            && (self.min_z < other.max_z && self.max_z > other.min_z)
    }

    pub fn contains(&self, position: &Vec3) -> bool {
        (self.min_x <= position.x && self.max_x >= position.x)
            && (self.min_y <= position.y && self.max_y >= position.y)
            && (self.min_z <= position.z && self.max_z >= position.z)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn max_diff_xz(&self, other: &Self, max_diff: f32) -> bool {
        (self.x - other.x).abs() <= max_diff && (self.z - other.z).abs() <= max_diff
    }

    pub fn max_diff_y(&self, other: &Self, max_diff: f32) -> bool {
        (self.y - other.y).abs() <= max_diff
    }
}

pub fn position_to_cell(map_bounds: &AABB, position: &Vec3) -> (usize, usize, usize) {
    (
        ((position.x - map_bounds.min_x) / CELL_SIZE).floor() as usize,
        ((position.y - map_bounds.min_y) / CELL_SIZE).floor() as usize,
        ((position.z - map_bounds.min_z) / CELL_SIZE).floor() as usize,
    )
}

pub fn cell_to_position(map_bounds: &AABB, cell: &(usize, usize, usize)) -> Vec3 {
    Vec3 {
        x: map_bounds.min_x + cell.0 as f32 * CELL_SIZE + CELL_SIZE / 2.0,
        y: map_bounds.min_y + cell.1 as f32 * CELL_SIZE + CELL_SIZE / 2.0,
        z: map_bounds.min_z + cell.2 as f32 * CELL_SIZE + CELL_SIZE / 2.0,
    }
}
