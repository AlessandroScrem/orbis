use std::path::PathBuf;

use crate::assets::IblId;

pub enum RuntimeEvent {
    Resize { width: u32, height: u32 },
    CloseRequested,
    DroppedFile(PathBuf),
    SetWindowTitle(String),
    UpdateIblMaps(IblId),
    ClearIblMaps,
    ReadbackSelection((u32, u32), (u32, u32)),
}
