pub mod rich;
pub mod span;
pub use iced_selection::{
    Text,
    selection::{Selection, SelectionEnd},
};
pub use iced_widget::core::text::{Alignment, LineHeight, Shaping, Wrapping};
pub use rich::SignalRich;
pub use span::SignalSpan;
