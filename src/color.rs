use crate::{
    math::vec4::Vec4,
    visitor::{Visitor, VisitResult, Visit}
};

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Color {
    // Do not change order! OpenGL requires this order!
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn opaque(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    pub fn white() -> Color {
        Color { r: 255, g: 255, b: 255, a: 255 }
    }

    pub fn black() -> Color {
        Color { r: 0, g: 0, b: 0, a: 255 }
    }

    pub fn as_frgba(self) -> Vec4 {
        Vec4 {
            x: f32::from(self.r) / 255.0,
            y: f32::from(self.g) / 255.0,
            z: f32::from(self.b) / 255.0,
            w: f32::from(self.a) / 255.0,
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let dr = (t * (i32::from(other.r) - i32::from(self.r)) as f32) as i32;
        let dg = (t * (i32::from(other.g) - i32::from(self.g)) as f32) as i32;
        let db = (t * (i32::from(other.b) - i32::from(self.b)) as f32) as i32;
        let da = (t * (i32::from(other.a) - i32::from(self.a)) as f32) as i32;

        let r = (i32::from(self.r) + dr) as u8;
        let g = (i32::from(self.g) + dg) as u8;
        let b = (i32::from(self.b) + db) as u8;
        let a = (i32::from(self.a) + da) as u8;

        Self {
            r,
            g,
            b,
            a,
        }
    }
}

impl Visit for Color {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;

        self.r.visit("R", visitor)?;
        self.g.visit("G", visitor)?;
        self.b.visit("B", visitor)?;
        self.a.visit("A", visitor)?;

        visitor.leave_region()
    }
}