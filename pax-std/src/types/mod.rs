use pax::*;
use pax::api::{PropertyInstance, PropertyLiteral, Interpolatable, SizePixels};


#[pax_type]
#[derive(Default, Clone)]
pub struct Stroke {
    pub color: Box<dyn PropertyInstance<Color>>,
    pub width: Box<dyn PropertyInstance<f64>>,
}

#[pax_type]
#[derive(Default, Clone)]
pub struct Text {
    pub content: Box<dyn PropertyInstance<String>>,
}


#[derive(Clone)]
#[pax_type]
pub struct StackerCellProperties {
    pub x_px: f64,
    pub y_px: f64,
    pub width_px: f64,
    pub height_px: f64,
}

/// Simple way to represent whether a stacker should render
/// vertically or horizontally
#[pax_type]
#[derive(Clone)]
pub enum StackerDirection {
    Vertical,
    Horizontal,
}

impl Default for StackerDirection {
    fn default() -> Self {
        StackerDirection::Horizontal
    }
}

impl Interpolatable for StackerDirection {}


#[pax_type]
#[derive(Clone)]
pub struct Font {
    pub family: Box<dyn pax::api::PropertyInstance<String>>,
    pub variant: Box<dyn pax::api::PropertyInstance<String>>,
    pub size: Box<dyn pax::api::PropertyInstance<SizePixels>>,
}
impl Into<FontMessage> for &Font {
    fn into(self) -> FontMessage {
        FontMessage {
             family: self.family.get().clone(),
             variant: self.variant.get().clone(),
             size: self.size.get().0,
        }
    }
}

impl PartialEq<FontMessage> for Font {
    fn eq(&self, other: &FontMessage) -> bool {

        self.family.get().eq(&other.family) &&
            self.variant.get().eq(&other.variant) &&
            self.size.get().eq(&other.size)

        //unequal if any of patch's fields are empty or if any
        //non-empty field does not match `self`'s stored values
        //
        // if matches!(&other.family, Some(family) if family.eq(self.family.get())) {
        //     //we good fam
        // } else {
        //     return false;
        // }
        //
        // if matches!(&other.variant, Some(variant) if variant.eq(self.variant.get())) {
        //     //we good fam
        // }else {
        //     return false;
        // }
        //
        // if matches!(&other.size, Some(size) if size.eq(self.size.get())) {
        //     //we good fam
        // }else {
        //     return false;
        // }

        // true
    }
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: Box::new(PropertyLiteral::new("Courier New".to_string())),
            variant: Box::new(PropertyLiteral::new("Regular".to_string())),
            size: Box::new(PropertyLiteral::new(SizePixels(14.0))),
        }
    }
}
impl Interpolatable for Font {}



#[pax_type]
#[derive(Clone)]
pub struct Color{
    pub color_variant: ColorVariant,
}
impl Default for Color {
    fn default() -> Self {
        Self {
            color_variant: ColorVariant::Rgba([0.0, 0.0, 1.0, 1.0])
        }
    }
}
// impl PartialEq<ColorRGBAPatch> for Color {
//     fn eq(&self, other: &ColorRGBAPatch) -> bool {
//         //unequal if any of patch's fields are empty or if any
//         //non-empty field does not match `self`'s stored values
//
//         if matches!(&other.family, Some(family) if family.eq(self.family.get())) {
//             //we good fam
//         } else {
//             return false;
//         }
//
//         if matches!(&other.variant, Some(variant) if variant.eq(self.variant.get())) {
//             //we good fam
//         }else {
//             return false;
//         }
//
//         if matches!(&other.size, Some(size) if size.eq(self.size.get())) {
//             //we good fam
//         }else {
//             return false;
//         }
//
//         true
//     }
// }

impl Interpolatable for Color {
    //TODO: Colors can be meaningfully interpolated.
}

impl Color {
    pub fn hlca(h:f64, l:f64, c:f64, a:f64) -> Self {
        Self {color_variant: ColorVariant::Hlca([h,l,c,a])}
    }
    pub fn rgba(r:f64, g:f64, b:f64, a:f64) -> Self {
        Self {color_variant: ColorVariant::Rgba([r,g,b,a])}
    }
    pub fn to_piet_color(&self) -> piet::Color {
        match self.color_variant {
            ColorVariant::Hlca(slice) => {
                piet::Color::hlca(slice[0], slice[1], slice[2], slice[3])
            },
            ColorVariant::Rgba(slice) => {
                piet::Color::rgba(slice[0], slice[1], slice[2], slice[3])
            }
        }
    }
}
#[pax_type]
#[derive(Clone)]
pub enum ColorVariant {
    Hlca([f64; 4]),
    Rgba([f64; 4]),
}

#[pax_type]
pub use pax::api::Size;
use pax_message::{ColorRGBAPatch, FontMessage};