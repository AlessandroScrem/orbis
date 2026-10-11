mod asset;
mod camera;
mod entity;
mod global;
mod material;
mod pick;
mod scene;
mod selection;

pub use asset::AssetCommand;
pub use camera::CameraCommand;
pub use entity::EntityCommand;
pub use global::GlobalCommand;
pub use material::MaterialCommand;
pub use pick::PickCommand;
pub use scene::SceneCommand;
pub use selection::SelectionCommand;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum EditorCommand {
    Scene(SceneCommand),
    Selection(SelectionCommand),
    Pick(PickCommand),
    Entity(EntityCommand),
    Asset(AssetCommand),
    Material(MaterialCommand),
    Camera(CameraCommand),
    Global(GlobalCommand),
    Exit,
}

impl From<EntityCommand> for EditorCommand {
    fn from(command: EntityCommand) -> Self {
        Self::Entity(command)
    }
}

impl From<AssetCommand> for EditorCommand {
    fn from(command: AssetCommand) -> Self {
        Self::Asset(command)
    }
}

impl From<MaterialCommand> for EditorCommand {
    fn from(command: MaterialCommand) -> Self {
        Self::Material(command)
    }
}

impl From<SelectionCommand> for EditorCommand {
    fn from(command: SelectionCommand) -> Self {
        Self::Selection(command)
    }
}

impl From<CameraCommand> for EditorCommand {
    fn from(command: CameraCommand) -> Self {
        Self::Camera(command)
    }
}
impl From<GlobalCommand> for EditorCommand {
    fn from(command: GlobalCommand) -> Self {
        Self::Global(command)
    }
}

impl EditorCommand {
    pub fn settings_changed(&self) -> bool {
        match self {
            Self::Global(command) => command.settings_changed(),
            Self::Camera(command) => command.settings_changed(),
            Self::Asset(command) => command.settings_changed(),
            _ => false,
        }
    }
}
