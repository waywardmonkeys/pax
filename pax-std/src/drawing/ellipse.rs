use kurbo::{Rect, Shape};
use pax_engine::*;
use pax_runtime::api::{use_RefCell, Stroke};
use pax_runtime::api::{Fill, Layer, RenderContext};
use pax_runtime::BaseInstance;
use pax_runtime::{ExpandedNode, InstanceFlags, InstanceNode, InstantiationArgs, RuntimeContext};
use_RefCell!();
use std::rc::Rc;

/// A basic 2D vector ellipse
#[pax]
#[engine_import_path("pax_engine")]
#[primitive("pax_std::drawing::ellipse::EllipseInstance")]
pub struct Ellipse {
    pub stroke: Property<Stroke>,
    pub fill: Property<Fill>,
}

pub struct EllipseInstance {
    base: BaseInstance,
}

impl InstanceNode for EllipseInstance {
    fn instantiate(args: InstantiationArgs) -> Rc<Self>
    where
        Self: Sized,
    {
        Rc::new(EllipseInstance {
            base: BaseInstance::new(
                args,
                InstanceFlags {
                    invisible_to_slot: false,
                    invisible_to_raycasting: false,
                    layer: Layer::Canvas,
                    is_component: false,
                },
            ),
        })
    }

    fn render(
        &self,
        expanded_node: &ExpandedNode,
        _context: &Rc<RuntimeContext>,
        rc: &mut dyn RenderContext,
    ) {
        let tab = expanded_node.transform_and_bounds.get();
        let (width, height) = tab.bounds;
        expanded_node.with_properties_unwrapped(|properties: &mut Ellipse| {
            let rect = Rect::from_points((0.0, 0.0), (width, height));
            let ellipse = kurbo::Ellipse::from_rect(rect);
            let accuracy = 0.1;
            let bez_path = ellipse.to_path(accuracy);

            let transformed_bez_path = Into::<kurbo::Affine>::into(tab.transform) * bez_path;
            let duplicate_transformed_bez_path = transformed_bez_path.clone();

            let color = if let Fill::Solid(properties_color) = properties.fill.get() {
                properties_color.to_piet_color()
            } else {
                unimplemented!("gradients not supported on ellipse")
            };

            let layer_id = format!("{}", expanded_node.occlusion.get().occlusion_layer_id);
            rc.fill(&layer_id, transformed_bez_path, &color.into());

            //hack to address "phantom stroke" bug on Web
            let width: f64 = properties
                .stroke
                .get()
                .width
                .get()
                .expect_pixels()
                .to_float();

            if width > f64::EPSILON {
                rc.stroke(
                    &layer_id,
                    duplicate_transformed_bez_path,
                    &properties.stroke.get().color.get().to_piet_color().into(),
                    width,
                );
            }
        });
    }

    fn resolve_debug(
        &self,
        f: &mut std::fmt::Formatter,
        expanded_node: Option<&ExpandedNode>,
    ) -> std::fmt::Result {
        match expanded_node {
            Some(expanded_node) => expanded_node
                .with_properties_unwrapped(|_e: &mut Ellipse| f.debug_struct("Ellipse").finish()),
            None => f.debug_struct("Ellipse").finish_non_exhaustive(),
        }
    }

    fn base(&self) -> &BaseInstance {
        &self.base
    }
}
