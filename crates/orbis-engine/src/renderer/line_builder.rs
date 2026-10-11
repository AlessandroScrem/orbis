use crate::assets::LinesVertexData;
use crate::math::*;
use crate::renderer::render_objects::LightRenderObject;

pub trait LineSink {
    fn line(&mut self, a: Vec3, b: Vec3, color: Vec3);
    fn arrow(&mut self, a: Vec3, b: Vec3, color: Vec3);
}

pub trait LineDrawable {
    fn emit(&self, sink: &mut dyn LineSink);
}

impl LineSink for Vec<LinesVertexData> {
    fn line(&mut self, a: Vec3, b: Vec3, color: Vec3) {
        self.push(LinesVertexData {
            position: a.into(),
            color: color.into(),
        });

        self.push(LinesVertexData {
            position: b.into(),
            color: color.into(),
        });
    }

    fn arrow(&mut self, a: Vec3, b: Vec3, color: Vec3) {
        // linea principale
        self.line(a, b, color);

        let dir = (b - a).normalize();
        let length = (b - a).magnitude();

        let head_len = length * 0.2;
        let head_width = head_len * 0.5;

        let tip = b;
        let base = b - dir * head_len;

        let mut side = dir.cross(Vec3::unit_y()).normalize();
        if side.magnitude2() < 0.001 {
            side = dir.cross(Vec3::unit_x()).normalize();
        }

        let left = base - side * head_width;
        let right = base + side * head_width;

        self.line(left, tip, color);
        self.line(right, tip, color);
    }
}

pub struct ObjectOrientedBoundingBox<'a> {
    pub bbox: &'a crate::BoundingBox,
    pub transform: &'a Mat4,
}
pub struct AxisAlignedBoundingBox<'a> {
    pub bbox: &'a crate::BoundingBox,
}

fn emit_box_edges(corners: &[Vec3; 8], color: Vec3, sink: &mut dyn LineSink) {
    #[rustfmt::skip]
    const BOX_EDGES: [(usize, usize); 12] = [
        (0, 1), (1, 2), (2, 3), (3, 0), 
        (4, 5), (5, 6), (6, 7), (7, 4), 
        (0, 4), (1, 5), (2, 6), (3, 7), 
    ];

    for (a, b) in BOX_EDGES {
        sink.line(corners[a], corners[b], color);
    }
}

impl<'a> LineDrawable for ObjectOrientedBoundingBox<'a> {
    fn emit(&self, sink: &mut dyn LineSink) {
        let color = crate::colors::CYAN_COLOR.into();
        let corners = self.bbox.gen_corners();

        let corners = corners.map(|c| (self.transform * c.extend(1.0)).truncate());

        emit_box_edges(&corners, color, sink);
    }
}

impl<'a> LineDrawable for AxisAlignedBoundingBox<'a> {
    fn emit(&self, sink: &mut dyn LineSink) {
        let color: Vec3 = crate::colors::CYAN_COLOR.into();
        let corners = self.bbox.gen_corners();

        emit_box_edges(&corners, color, sink);
    }
}

impl LineDrawable for LightRenderObject {
    fn emit(&self, sink: &mut dyn LineSink) {
        use crate::colors;

        let mut corners = [
            Vec3::new(-1.0, -1.0, 0.0), // Near-bottom-left
            Vec3::new(1.0, -1.0, 0.0),  // Near-bottom-right
            Vec3::new(1.0, 1.0, 0.0),   // Near-top-right
            Vec3::new(-1.0, 1.0, 0.0),  // Near-top-left
            Vec3::new(-1.0, -1.0, 1.0), // Far-bottom-left
            Vec3::new(1.0, -1.0, 1.0),  // Far-bottom-right
            Vec3::new(1.0, 1.0, 1.0),   // Far-top-right
            Vec3::new(-1.0, 1.0, 1.0),  // Far-top-left
        ];

        let mat = self.get_view_proj_matrix();
        let inverse_light_space_matrix = mat.invert().unwrap_or(Mat4::identity());
        for vertex in corners.iter_mut() {
            let v = inverse_light_space_matrix * vertex.extend(1.0);
            *vertex = v.truncate();
        }

        let near = [corners[0], corners[1], corners[2], corners[3]];
        let far = [corners[4], corners[5], corners[6], corners[7]];

        // Near clip
        sink.line(near[0], near[1], colors::RED_COLOR.into());
        sink.line(near[1], near[2], colors::RED_COLOR.into());
        sink.line(near[2], near[3], colors::RED_COLOR.into());
        sink.line(near[3], near[0], colors::RED_COLOR.into());
        // Far clip
        sink.line(far[0], far[1], colors::BLUE_COLOR.into());
        sink.line(far[1], far[2], colors::BLUE_COLOR.into());
        sink.line(far[2], far[3], colors::BLUE_COLOR.into());
        sink.line(far[3], far[0], colors::BLUE_COLOR.into());
        // Linees connecting near
        sink.line(near[0], far[0], colors::GREEN_COLOR.into());
        sink.line(near[1], far[1], colors::GREEN_COLOR.into());
        sink.line(near[2], far[2], colors::GREEN_COLOR.into());
        sink.line(near[3], far[3], colors::GREEN_COLOR.into());

        let origin = Vec3::new(0.0, 0.0, 0.0);
        let position = Vec3::new(self.position.x, self.position.y, self.position.z);
        let direction = (origin - position).normalize();
        let target = position + direction * 20.0;

        sink.arrow(position, target, colors::GREEN_COLOR.into());
    }
}
