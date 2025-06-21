use bytes::Bytes;
use iced::widget::image::Handle;
use std::{fmt, io};

#[derive(Clone)]
pub struct Avatar {
    pub width: u32,
    pub height: u32,
    pub rgba: Bytes,
}

impl Avatar {
    const WIDTH: u32 = 50;
    const HEIGHT: u32 = 50;

    pub fn from_bytes(bytes: impl Into<Bytes>) -> Option<Self> {
        let mut image = image::ImageReader::new(io::Cursor::new(bytes.into()))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?
            .thumbnail(Self::WIDTH, Self::HEIGHT)
            .into_rgba8();

        circular_crop(&mut image);

        Some(Self {
            width: image.width(),
            height: image.height(),
            rgba: Bytes::from(image.into_raw()),
        })
    }

    pub fn into_handle(self) -> Handle {
        Handle::from_rgba(self.width, self.height, self.rgba)
    }
}

impl fmt::Debug for Avatar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba", &self.rgba.len())
            .finish()
    }
}

fn circular_crop(rgba: &mut image::RgbaImage) {
    let (width, height) = rgba.dimensions();
    if width != height {
        return;
    }

    let radius = (width as f32 * 0.5) as u32;
    let radius_sq = radius * radius;
    let aa_span = radius / 4;

    for y in 0..height {
        for x in 0..width {
            let dist_x = if x < radius {
                radius - x
            } else if x >= width - radius {
                x - (width - radius - 1)
            } else {
                0
            };

            let dist_y = if y < radius {
                radius - y
            } else if y >= height - radius {
                y - (height - radius - 1)
            } else {
                0
            };

            let dist_sq = dist_x * dist_x + dist_y * dist_y;

            if dist_sq > radius_sq {
                let dist = (dist_sq as f32).sqrt();

                if dist <= (radius + aa_span) as f32 {
                    let alpha_scale =
                        1.0 - (dist_sq - radius_sq) as f32 / (aa_span * aa_span) as f32;

                    let pixel = rgba.get_pixel_mut(x, y);
                    pixel.0[3] = (f32::from(pixel.0[3]) * alpha_scale) as u8;
                } else {
                    let pixel = rgba.get_pixel_mut(x, y);
                    pixel.0 = [0; 4];
                }
            }
        }
    }
}
