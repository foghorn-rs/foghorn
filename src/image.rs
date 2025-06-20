use bytes::Bytes;
use iced::widget::image::Handle;
use std::{fmt, io};
use tokio::task;

#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Bytes,
}

impl Image {
    const WIDTH: u32 = 50;
    const HEIGHT: u32 = 50;

    pub fn from_bytes<'a>(
        bytes: impl Into<Bytes>,
    ) -> impl Future<Output = Result<Self, anywho::Error>> + 'a {
        let bytes = bytes.into();

        async move {
            task::spawn_blocking(move || {
                let mut image = image::ImageReader::new(io::Cursor::new(bytes))
                    .with_guessed_format()?
                    .decode()?
                    .thumbnail(Self::WIDTH, Self::HEIGHT)
                    .into_rgba8();

                if !is_circular(&image) {
                    circular_crop(&mut image);
                }

                Ok(Self {
                    width: image.width(),
                    height: image.height(),
                    rgba: Bytes::from(image.into_raw()),
                })
            })
            .await?
        }
    }

    pub fn into_handle(self) -> Handle {
        Handle::from_rgba(self.width, self.height, self.rgba)
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba", &self.rgba.len())
            .finish()
    }
}

fn is_circular(rgba: &image::RgbaImage) -> bool {
    let (width, height) = rgba.dimensions();
    if width != height {
        return false;
    }

    let radius = width as f32 / 2.0;
    let center = (radius, radius);

    let check_coords = [
        (0, 0),
        (49, 0),
        (0, 49),
        (49, 49),
        (0, 25),
        (25, 0),
        (49, 25),
        (25, 49),
    ];

    for &(x, y) in &check_coords {
        let dx = x as f32 + 0.5 - center.0;
        let dy = y as f32 + 0.5 - center.1;
        let dist_sq = dx.mul_add(dx, dy * dy);

        if dist_sq > radius * radius {
            let pixel = rgba.get_pixel(x, y);
            if pixel.0[3] != 0 {
                return false;
            }
        }
    }

    true
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
