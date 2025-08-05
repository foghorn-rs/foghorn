pub static LUCIDE_BYTES: &[u8] = include_bytes!("../Lucide.ttf");
pub static LUCIDE_FONT: iced::Font = iced::Font::with_name("lucide");

macro_rules! icon {
    ($name:ident = $icon:literal) => {
        pub fn $name<'a>() -> ::iced::widget::Text<'a> {
            ::iced::widget::text(const { ::core::char::from_u32($icon).unwrap() })
                .font(LUCIDE_FONT)
                .line_height(1.0)
        }
    };
}

// https://unpkg.com/lucide-static@latest/font/info.json
icon!(reply = 57898);
icon!(edit = 57849);
