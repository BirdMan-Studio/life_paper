pub mod auth;
pub mod creatures;
pub mod models;
pub mod settings;
pub mod world;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Auth,
    World,
    Creatures,
    Models,
    Settings,
}
