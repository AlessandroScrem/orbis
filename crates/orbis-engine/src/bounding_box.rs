use crate::math::{Mat4, Vec3};
const CORNERS: usize = 8;
pub type BBoxCornerData = [Vec3; CORNERS];

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self::new_empty()
    }
}

// Implementazione di From per tuple di array ([f32;3], [f32;3])
impl From<([f32; 3], [f32; 3])> for BoundingBox {
    fn from(value: ([f32; 3], [f32; 3])) -> Self {
        BoundingBox {
            min: value.0,
            max: value.1,
        }
    }
}

impl BoundingBox {
    pub fn new_empty() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    pub fn extend(&mut self, point: &[f32; 3]) {
        for ((mi, ma), &val) in self.min.iter_mut().zip(self.max.iter_mut()).zip(point) {
            *mi = mi.min(val);
            *ma = ma.max(val);
        }
    }

    pub fn merge(&mut self, other: &BoundingBox) {
        self.extend(&other.min);
        self.extend(&other.max);
    }

    #[allow(dead_code)]
    pub fn from_points<'a, I: IntoIterator<Item = &'a [f32; 3]>>(points: I) -> Self {
        let mut bbox = BoundingBox::new_empty();
        for p in points {
            bbox.extend(p);
        }
        bbox
    }
}

impl BoundingBox {
    pub fn gen_corners(&self) -> BBoxCornerData {
        /*
        bbox vertices order:
            y  7----------6
            | /|         /|
            |/ |        / |
            3----------2  |
            |  | z     |  |
            |  4-------|--5
            | /        | /
            |/         |/
            0----------1 --->x
        */
        [
            Vec3::new(self.min[0], self.min[1], self.min[2]),
            Vec3::new(self.max[0], self.min[1], self.min[2]),
            Vec3::new(self.max[0], self.max[1], self.min[2]),
            Vec3::new(self.min[0], self.max[1], self.min[2]),
            Vec3::new(self.min[0], self.min[1], self.max[2]),
            Vec3::new(self.max[0], self.min[1], self.max[2]),
            Vec3::new(self.max[0], self.max[1], self.max[2]),
            Vec3::new(self.min[0], self.max[1], self.max[2]),
        ]
    }

    pub fn transform_aabb(&self, matrix: &Mat4) -> Self {
        let corners = self.gen_corners();

        // Trasformazione
        let transformed = corners.map(|c| matrix * c.extend(1.0));

        // Ricostruzione AABB
        let mut bbox = Self::new_empty();
        for p in transformed {
            bbox.extend(&p.truncate().into());
        }
        bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_bounding_box_from_points() {
        let points: Vec<[f32; 3]> = vec![
            [1.0, 5.0, 3.0],  //
            [2.0, -1.0, 7.0], //
            [0.5, 4.0, 6.0],
        ];

        let aabb = BoundingBox::from_points(points.iter());

        assert_eq!(aabb.min, [0.5, -1.0, 3.0]);
        assert_eq!(aabb.max, [2.0, 5.0, 7.0]);
    }

    #[test]
    fn should_bounding_box_from_single_point() {
        let points = [[1.0, 2.0, 3.0]];

        let aabb = BoundingBox::from_points(points.iter());

        assert_eq!(aabb.min, [1.0, 2.0, 3.0]);
        assert_eq!(aabb.max, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_aabb_empty() {
        let bbox = BoundingBox::new_empty();

        // min rimane INFINITY, max rimane NEG_INFINITY
        assert_eq!(bbox.min, [f32::INFINITY; 3]);
        assert_eq!(bbox.max, [f32::NEG_INFINITY; 3]);
    }

    #[test]
    fn test_extend_step_by_step() {
        let mut bbox = BoundingBox::new_empty();

        // dopo il primo punto
        bbox.extend(&[1.0, 2.0, 3.0]);
        assert_eq!(bbox.min, [1.0, 2.0, 3.0]);
        assert_eq!(bbox.max, [1.0, 2.0, 3.0]);

        // aggiungo un punto "più piccolo"
        bbox.extend(&[0.0, -1.0, 2.5]);
        assert_eq!(bbox.min, [0.0, -1.0, 2.5]);
        assert_eq!(bbox.max, [1.0, 2.0, 3.0]);

        // aggiungo un punto "più grande"
        bbox.extend(&[5.0, 3.0, 10.0]);
        assert_eq!(bbox.min, [0.0, -1.0, 2.5]);
        assert_eq!(bbox.max, [5.0, 3.0, 10.0]);
    }
}
