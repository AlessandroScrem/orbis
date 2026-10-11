
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum GlobalCommand {
    SetLightEnable(bool),
    SetIblEnable(bool),
    SetSkyboxEnable(bool),
    SetSkyboxBlur(bool),
    SetAxisEnable(bool),
    SetBoundingBoxEnable(bool),
    SetBoundingBoxAxisAligned(bool),
    SetMipsWithCompute(bool),
    SetEnvironmentRotation(f32),
    SetDebugCode(u32),
    SetExposure(f32),
    SetIblIntensity(f32),
    SetTonemap(u32),
}

impl GlobalCommand {
    pub fn settings_changed(&self) -> bool {
        matches!(
            self,
            Self::SetIblEnable(_)
                | Self::SetLightEnable(_)
                | Self::SetSkyboxEnable(_)
                | Self::SetSkyboxBlur(_)
                | Self::SetAxisEnable(_)
                | Self::SetBoundingBoxEnable(_)
                | Self::SetBoundingBoxAxisAligned(_)
                | Self::SetMipsWithCompute(_)
                | Self::SetEnvironmentRotation(_)
                | Self::SetDebugCode(_)
                | Self::SetExposure(_)
                | Self::SetIblIntensity(_)
                | Self::SetTonemap(_)
        )
    }
}
