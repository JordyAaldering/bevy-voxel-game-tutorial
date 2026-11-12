#[derive(Clone, Copy, PartialEq, Eq)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
#[repr(transparent)]
pub struct Density(u8);

impl Density {
    pub const EMPTY: Self = Self(u8::MIN);

    pub const FULL: Self = Self(u8::MAX);
}

impl Into<f32> for Density {
    fn into(self) -> f32 {
        self.0 as f32 / u8::MAX as f32
    }
}
