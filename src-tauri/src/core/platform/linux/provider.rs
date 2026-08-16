#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowObservation {
    pub pid: Option<u32>,
    pub app_id: String,
    pub wm_class: String,
    pub title: String,
}

pub trait ToplevelProvider {
    const NAME: &'static str;

    fn probe() -> Option<Self>
    where
        Self: Sized;

    fn run(self, emit: &mut dyn FnMut(WindowObservation));
}
