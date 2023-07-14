use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env::Args;
use std::rc::Rc;
use std::thread::sleep;
use std::time::Duration;
use kurbo::Point;

use pax_message::{LayerAddPatch, NativeMessage};

use piet_common::RenderContext;

use crate::{Affine, ComponentInstance, Color, ComputableTransform, RenderNodePtr, ExpressionContext, RenderNodePtrList, RenderNode, TabCache, TransformAndBounds, StackFrame, ScrollerArgs};
use crate::runtime::{Runtime};
use pax_properties_coproduct::{PropertiesCoproduct, TypesCoproduct};
use pax_message::NativeMessage::LayerAdd;

use pax_runtime_api::{ArgsClick, ArgsJab, ArgsScroll, ArgsTouchStart, ArgsTouchMove, ArgsTouchEnd, ArgsKeyDown, ArgsKeyUp, ArgsKeyPress, ArgsMouseDown, ArgsMouseUp, ArgsMouseOver, ArgsMouseOut, ArgsDoubleClick, ArgsContextMenu, ArgsWheel, Interpolatable, TransitionManager, Layer, LayerInfo, RuntimeContext, ArgsMouseMove};

pub struct PaxEngine<R: 'static + RenderContext> {
    pub frames_elapsed: usize,
    pub instance_registry: Rc<RefCell<InstanceRegistry<R>>>,
    pub expression_table: HashMap<usize, Box<dyn Fn(ExpressionContext<R>) -> TypesCoproduct> >,
    pub main_component: Rc<RefCell<ComponentInstance<R>>>,
    pub runtime: Rc<RefCell<Runtime<R>>>,
    pub image_map: HashMap<Vec<u64>, (Box<Vec<u8>>, usize, usize)>,
    viewport_tab: TransformAndBounds,
}

pub struct ExpressionVTable<R: 'static + RenderContext> {
    inner_map: HashMap<usize, Box<dyn Fn(ExpressionContext<R>) -> TypesCoproduct>>,
    dependency_graph: HashMap<u64, Vec<u64>>,
}

pub struct RenderTreeContext<'a, R: 'static + RenderContext>
{
    pub engine: &'a PaxEngine<R>,
    pub transform: Affine,
    pub bounds: (f64, f64),
    pub runtime: Rc<RefCell<Runtime<R>>>,
    pub node: RenderNodePtr<R>,
    pub parent_repeat_expanded_node: Option<Rc<RepeatExpandedNode<R>>>,
    pub timeline_playhead_position: usize,
    pub inherited_adoptees: Option<RenderNodePtrList<R>>,
}


impl<'a, R: 'static + RenderContext> RenderTreeContext<'a, R> {
    pub fn distill_userland_node_context(&self) -> RuntimeContext {
        RuntimeContext {
            bounds_parent: self.bounds,
            frames_elapsed: self.engine.frames_elapsed,
        }
    }
}

impl<'a, R: 'static + RenderContext> Clone for RenderTreeContext<'a, R> {
    fn clone(&self) -> Self {
        RenderTreeContext {
            engine: &self.engine,
            transform: self.transform.clone(),
            bounds: self.bounds.clone(),
            runtime: Rc::clone(&self.runtime),
            node: Rc::clone(&self.node),
            parent_repeat_expanded_node: self.parent_repeat_expanded_node.clone(),
            timeline_playhead_position: self.timeline_playhead_position.clone(),
            inherited_adoptees: self.inherited_adoptees.clone(),
        }
    }
}

impl<'a, R: RenderContext> RenderTreeContext<'a, R> {
    pub fn compute_eased_value<T: Clone + Interpolatable>(&self, transition_manager: Option<&mut TransitionManager<T>>) -> Option<T> {
        if let Some(mut tm) = transition_manager {
            if tm.queue.len() > 0 {
                let mut current_transition = tm.queue.get_mut(0).unwrap();
                if let None = current_transition.global_frame_started {
                    current_transition.global_frame_started = Some(self.engine.frames_elapsed);
                }
                let progress = (self.engine.frames_elapsed as f64 - current_transition.global_frame_started.unwrap() as f64) / (current_transition.duration_frames as f64);
                return if progress >= 1.0 { //NOTE: we may encounter float imprecision here, consider `progress >= 1.0 - EPSILON` for some `EPSILON`
                    let new_value = current_transition.curve.interpolate(&current_transition.starting_value, &current_transition.ending_value, progress);
                    tm.value = Some(new_value.clone());

                    tm.queue.pop_front();
                    self.compute_eased_value(Some(tm))
                } else {
                    let new_value = current_transition.curve.interpolate(&current_transition.starting_value, &current_transition.ending_value, progress);
                    tm.value = Some(new_value.clone());
                    tm.value.clone()
                };
            } else {
                return tm.value.clone();
            }
        }
        None
    }

    /// Get an `id_chain` for this element, an array of `u64` used collectively as a single unique ID across native bridges.
    /// Specifically, the ID chain represents not only the instance ID, but the indices of each RepeatItem found by a traversal
    /// of the runtime stack.
    ///
    /// The need for this emerges from the fact that `Repeat`ed elements share a single underlying
    /// `instance`, where that instantiation happens once at init-time — specifically, it does not happen
    /// when `Repeat`ed elements are added and removed to the render tree.  10 apparent rendered elements may share the same `instance_id` -- which doesn't work as a unique key for native renderers
    /// that are expected to render and update 10 distinct elements.
    ///
    /// Thus, the `id_chain` is used as a unique key, first the `instance_id` (which will increase monotonically through the lifetime of the program),
    /// then each RepeatItem index through a traversal of the stack frame.  Thus, each virtually `Repeat`ed element
    /// gets its own unique ID in the form of an "address" through any nested `Repeat`-ancestors.
    pub fn get_id_chain(&self, id: u64) -> Vec<u64> {
        let mut indices = (*self.runtime).borrow().get_list_of_repeat_indicies_from_stack();
        indices.insert(0, id);
        indices
    }

    pub fn compute_vtable_value(&self, vtable_id: Option<usize>) -> Option<TypesCoproduct> {

        if let Some(id) = vtable_id {
            if let Some(evaluator) = self.engine.expression_table.get(&id) {
                let ec = ExpressionContext {
                    engine: self.engine,
                    stack_frame: Rc::clone(&(*self.runtime).borrow_mut().peek_stack_frame().unwrap()),
                };
                return Some((**evaluator)(ec));
            }
        } //FUTURE: for timelines: else if present in timeline vtable...

        None
    }
}

pub struct HandlerRegistry<R: 'static + RenderContext> {
    pub scroll_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsScroll)>,
    pub jab_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsJab)>,
    pub touch_start_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsTouchStart)>,
    pub touch_move_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsTouchMove)>,
    pub touch_end_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsTouchEnd)>,
    pub key_down_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsKeyDown)>,
    pub key_up_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsKeyUp)>,
    pub key_press_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsKeyPress)>,
    pub click_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsClick)>,
    pub mouse_down_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsMouseDown)>,
    pub mouse_up_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsMouseUp)>,
    pub mouse_move_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsMouseMove)>,
    pub mouse_over_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsMouseOver)>,
    pub mouse_out_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsMouseOut)>,
    pub double_click_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsDoubleClick)>,
    pub context_menu_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsContextMenu)>,
    pub wheel_handlers: Vec<fn(Rc<RefCell<StackFrame<R>>>, RuntimeContext, ArgsWheel)>,
    pub will_render_handlers: Vec<fn(Rc<RefCell<PropertiesCoproduct>>, RuntimeContext)>,
    pub did_mount_handlers: Vec<fn(Rc<RefCell<PropertiesCoproduct>>, RuntimeContext)>,
}


impl<R: 'static + RenderContext> Default for HandlerRegistry<R> {
    fn default() -> Self {
        HandlerRegistry {
            scroll_handlers: Vec::new(),
            jab_handlers: Vec::new(),
            touch_start_handlers: Vec::new(),
            touch_move_handlers: Vec::new(),
            touch_end_handlers: Vec::new(),
            key_down_handlers: Vec::new(),
            key_up_handlers: Vec::new(),
            key_press_handlers: Vec::new(),
            click_handlers: Vec::new(),
            mouse_down_handlers: Vec::new(),
            mouse_up_handlers: Vec::new(),
            mouse_move_handlers: Vec::new(),
            mouse_over_handlers: Vec::new(),
            mouse_out_handlers: Vec::new(),
            double_click_handlers: Vec::new(),
            context_menu_handlers: Vec::new(),
            wheel_handlers: Vec::new(),
            will_render_handlers: Vec::new(),
            did_mount_handlers: Vec::new(),
        }
    }
}

/// Represents a repeat-expanded node.  For example, a Rectangle inside `for i in 0..3` and
/// a `for j in 0..4` would have 12 repeat-expanded nodes representing the 12 virtual Rectangles in the
/// rendered scene graph. These nodes are addressed uniquely by id_chain (see documentation for `get_id_chain`.)
pub struct RepeatExpandedNode<R: 'static + RenderContext> {
    id_chain: Vec<u64>,
    parent_repeat_expanded_node: Option<Rc<RepeatExpandedNode<R>>>,
    instance_node: RenderNodePtr<R>,
    stack_frame: Rc<RefCell<crate::StackFrame<R>>>,
    tab: TransformAndBounds,
    node_context: RuntimeContext,
}

impl<R: 'static + RenderContext> RepeatExpandedNode<R> {
    pub fn dispatch_scroll(&self, args_scroll: ArgsScroll) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().scroll_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_scroll.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_scroll(args_scroll);
        }
    }

    pub fn dispatch_jab(&self, args_jab: ArgsJab) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().jab_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_jab.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_jab(args_jab);
        }
    }

    pub fn dispatch_touch_start(&self, args_touch_start: ArgsTouchStart) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().touch_start_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_touch_start.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_touch_start(args_touch_start);
        }
    }

    pub fn dispatch_touch_move(&self, args_touch_move: ArgsTouchMove) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().touch_move_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_touch_move.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_touch_move(args_touch_move);
        }
    }

    pub fn dispatch_touch_end(&self, args_touch_end: ArgsTouchEnd) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().touch_end_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_touch_end.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_touch_end(args_touch_end);
        }
    }

    pub fn dispatch_key_down(&self, args_key_down: ArgsKeyDown) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().key_down_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_key_down.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_key_down(args_key_down);
        }
    }

    pub fn dispatch_key_up(&self, args_key_up: ArgsKeyUp) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().key_up_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_key_up.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_key_up(args_key_up);
        }
    }

    pub fn dispatch_key_press(&self, args_key_press: ArgsKeyPress) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().key_press_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_key_press.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_key_press(args_key_press);
        }
    }

    pub fn dispatch_click(&self, args_click: ArgsClick) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().click_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_click.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_click(args_click);
        }
    }

    pub fn dispatch_mouse_down(&self, args_mouse_down: ArgsMouseDown) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().mouse_down_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_mouse_down.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_mouse_down(args_mouse_down);
        }
    }

    pub fn dispatch_mouse_up(&self, args_mouse_up: ArgsMouseUp) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().mouse_up_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_mouse_up.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_mouse_up(args_mouse_up);
        }
    }

    pub fn dispatch_mouse_move(&self, args_mouse_move: ArgsMouseMove) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().mouse_move_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_mouse_move.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_mouse_move(args_mouse_move);
        }
    }

    pub fn dispatch_mouse_over(&self, args_mouse_over: ArgsMouseOver) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().mouse_over_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_mouse_over.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_mouse_over(args_mouse_over);
        }
    }

    pub fn dispatch_mouse_out(&self, args_mouse_out: ArgsMouseOut) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().mouse_out_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_mouse_out.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_mouse_out(args_mouse_out);
        }
    }

    pub fn dispatch_double_click(&self, args_double_click: ArgsDoubleClick) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().double_click_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_double_click.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_double_click(args_double_click);
        }
    }

    pub fn dispatch_context_menu(&self, args_context_menu: ArgsContextMenu) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().context_menu_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_context_menu.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_context_menu(args_context_menu);
        }
    }

    pub fn dispatch_wheel(&self, args_wheel: ArgsWheel) {
        if let Some(registry) = (*self.instance_node).borrow().get_handler_registry() {
            let handlers = &(*registry).borrow().wheel_handlers;
            handlers.iter().for_each(|handler| {
                handler(Rc::clone(&self.stack_frame), self.node_context.clone(), args_wheel.clone());
            });
        }

        if let Some(parent) = &self.parent_repeat_expanded_node {
            parent.dispatch_wheel(args_wheel);
        }
    }
}



pub struct InstanceRegistry<R: 'static + RenderContext> {
    ///look up RenderNodePtr by id
    instance_map: HashMap<u64, RenderNodePtr<R>>,

    ///a cache of repeat-expanded elements visited by rendertree traversal,
    ///intended to be cleared at the beginning of each frame and populated
    ///with each node visited.  This enables post-facto operations on nodes with
    ///otherwise ephemeral calculations, e.g. the descendants of `Repeat` instances.
    repeat_expanded_node_cache: Vec<Rc<RepeatExpandedNode<R>>>,

    ///track which repeat-expanded elements are currently mounted -- if id is present in set, is mounted
    mounted_set: HashSet<Vec<u64>>,
    ///tracks whichs instance nodes are marked for unmounting, to be done at the correct point in the render tree lifecycle
    marked_for_unmount_set: HashSet<u64>,

    ///register holding the next value to mint as an id
    next_id: u64,
}

impl<R: 'static + RenderContext> InstanceRegistry<R> {
    pub fn new() -> Self {
        Self {
            mounted_set: HashSet::new(),
            marked_for_unmount_set: HashSet::new(),
            instance_map: HashMap::new(),
            repeat_expanded_node_cache: vec![],
            next_id: 0,
        }
    }

    pub fn mint_id(&mut self) -> u64 {
        let new_id = self.next_id;
        self.next_id = self.next_id + 1;
        new_id
    }

    pub fn register(&mut self, instance_id: u64, node: RenderNodePtr<R>) {
        self.instance_map.insert(instance_id, node);
    }

    pub fn deregister(&mut self, instance_id: u64) {
        self.instance_map.remove(&instance_id);
    }

    pub fn mark_mounted(&mut self, id_chain: Vec<u64>) {
        self.mounted_set.insert(id_chain);
    }

    pub fn is_mounted(&self, id_chain: &Vec<u64>) -> bool {
        self.mounted_set.contains(id_chain)
    }

    pub fn mark_for_unmount(&mut self, instance_id: u64) {
        self.marked_for_unmount_set.insert(instance_id);
    }

    pub fn reset_repeat_expanded_node_cache(&mut self) {
        self.repeat_expanded_node_cache = vec![];
    }

    pub fn add_to_repeat_expanded_node_cache(&mut self, repeat_expanded_node: Rc<RepeatExpandedNode<R>>) {
        //Note: ray-casting requires that these nodes are sorted by z-index
        self.repeat_expanded_node_cache.push(repeat_expanded_node);
    }

}

impl<R: 'static + RenderContext> PaxEngine<R> {
    pub fn new(
        main_component_instance: Rc<RefCell<ComponentInstance<R>>>,
        expression_table: HashMap<usize, Box<dyn Fn(ExpressionContext<R>)->TypesCoproduct>>,
        logger: pax_runtime_api::PlatformSpecificLogger,
        viewport_size: (f64, f64),
        instance_registry: Rc<RefCell<InstanceRegistry<R>>>,
    ) -> Self {
        pax_runtime_api::register_logger(logger);
        PaxEngine {
            frames_elapsed: 0,
            instance_registry,
            expression_table,
            runtime: Rc::new(RefCell::new(Runtime::new())),
            main_component: main_component_instance,
            viewport_tab: TransformAndBounds {
                transform: Affine::default(),
                bounds: viewport_size,
            },
            image_map: HashMap::new(),
        }
    }

    fn traverse_render_tree(&self, rcs: &mut Vec<R>) -> Vec<pax_message::NativeMessage> {
        //Broadly:
        // 1. compute properties
        // 2. find lowest node (last child of last node), accumulating transform along the way
        // 3. start rendering, from lowest node on-up

        let cast_component_rc : RenderNodePtr<R> = self.main_component.clone();

        let mut rtc = RenderTreeContext {
            engine: &self,
            transform: Affine::default(),
            bounds: self.viewport_tab.bounds,
            runtime: self.runtime.clone(),
            node: Rc::clone(&cast_component_rc),
            parent_repeat_expanded_node: None,
            timeline_playhead_position: self.frames_elapsed,
            inherited_adoptees: None,
        };

        let mut depth = LayerInfo::new();
        self.recurse_traverse_render_tree(&mut rtc, rcs, Rc::clone(&cast_component_rc), &mut depth, false);
        //reset the marked_for_unmount set
        self.instance_registry.borrow_mut().marked_for_unmount_set = HashSet::new();

        if depth.get_depth() >= rcs.len() {
            let layerAddPatch = LayerAddPatch {
                num_layers_to_add: depth.get_depth()-rcs.len()+1,
            };
            (*self.runtime).borrow_mut().enqueue_native_message(LayerAdd(layerAddPatch));
        }

        // here I have depth, compare with rcs length and if rcs less than depth add native message to chassis to increase layers for next tick
        let native_render_queue = (*self.runtime).borrow_mut().take_native_message_queue();
        native_render_queue.into()
    }

    fn recurse_traverse_render_tree(&self, rtc: &mut RenderTreeContext<R>, rcs: &mut Vec<R>, node: RenderNodePtr<R>, layer_info: &mut LayerInfo, marked_for_unmount: bool)  {
        //Recurse:
        //  - compute properties for this node
        //  - fire lifecycle events for this node
        //  - iterate backwards over children (lowest first); recurse until there are no more descendants.  track transform matrix & bounding dimensions along the way.
        //  - we now have the back-most leaf node.  Render it.  Return.
        //  - we're now at the second back-most leaf node.  Render it.  Return ...
        //  - manage unmounting, if marked

        //populate a pointer to this (current) `RenderNode` onto `rtc`
        rtc.node = Rc::clone(&node);

        //lifecycle: compute_properties happens before rendering
        node.borrow_mut().compute_properties(rtc);
        let accumulated_transform = rtc.transform;
        let accumulated_bounds = rtc.bounds;


        //fire `did_mount` event if this is this node's first frame
        //Note that this must happen after initial `compute_properties`, which performs the
        //necessary side-effect of creating the `self` that must be passed to handlers
        {
            let id = (*rtc.node).borrow().get_instance_id();
            let mut instance_registry = (*rtc.engine.instance_registry).borrow_mut();

            //Due to Repeat, an effective unique instance ID is the tuple: `(instance_id, [list_of_RepeatItem_indices])`
            let mut repeat_indices = (*rtc.engine.runtime).borrow().get_list_of_repeat_indicies_from_stack();
            let id_chain = {let mut i = vec![id]; i.append(&mut repeat_indices); i};
            if !instance_registry.is_mounted(&id_chain) {
                //Fire primitive-level did_mount lifecycle method
                node.borrow_mut().handle_did_mount(rtc);

                //Fire registered did_mount events
                let registry = (*node).borrow().get_handler_registry();
                if let Some(registry) = registry {
                    //grab Rc of properties from stack frame; pass to type-specific handler
                    //on instance in order to dispatch cartridge method
                    match rtc.runtime.borrow_mut().peek_stack_frame() {
                        Some(stack_frame) => {
                            for handler in (*registry).borrow().did_mount_handlers.iter() {
                                handler(stack_frame.borrow_mut().get_properties(), rtc.distill_userland_node_context());
                            }
                        },
                        None => {

                        },
                    }
                }
                instance_registry.mark_mounted(id_chain);
            }
        }

        //peek at the current stack frame and set a scoped playhead position as needed
        match rtc.runtime.borrow_mut().peek_stack_frame() {
            Some(stack_frame) => {
                rtc.timeline_playhead_position = stack_frame.borrow_mut().get_timeline_playhead_position().clone();
            },
            None => ()
        }

        //get the size of this node (calc'd or otherwise) and use
        //it as the new accumulated bounds: both for this nodes children (their parent container bounds)
        //and for this node itself (e.g. for specifying the size of a Rectangle node)
        let new_accumulated_bounds = node.borrow_mut().compute_size_within_bounds(accumulated_bounds);
        let mut node_size : (f64, f64) = (0.0, 0.0);
        let node_computed_transform = {
            let mut node_borrowed = rtc.node.borrow_mut();
            node_size = node_borrowed.compute_size_within_bounds(accumulated_bounds);
            let components = node_borrowed.get_transform().borrow_mut().get()
            .compute_transform_matrix(
                node_size,
                accumulated_bounds,
            );
            //combine align transformation exactly once per element per frame
            components.1 * components.0
        };

        let new_accumulated_transform = accumulated_transform * node_computed_transform;
        rtc.bounds = new_accumulated_bounds.clone();
        rtc.transform = new_accumulated_transform.clone();

        //lifecycle: will_render for primitives
        node.borrow_mut().handle_will_render(rtc, rcs);

        //fire `will_render` handlers
        let registry = (*node).borrow().get_handler_registry();
        if let Some(registry) = registry {
            //grab Rc of properties from stack frame; pass to type-specific handler
            //on instance in order to dispatch cartridge method
            match rtc.runtime.borrow_mut().peek_stack_frame() {
                Some(stack_frame) => {
                    for handler in (*registry).borrow().will_render_handlers.iter() {
                        handler(stack_frame.borrow_mut().get_properties(), rtc.distill_userland_node_context());
                    }
                },
                None => {
                    panic!("can't bind events without a component")
                },
            }
        }

        //create the `repeat_expanded_node` for the current node
        let children = node.borrow_mut().get_rendering_children();
        let id_chain = rtc.get_id_chain(node.borrow().get_instance_id());
        let repeat_expanded_node_tab = TransformAndBounds {
            bounds: node_size,
            transform: new_accumulated_transform.clone(),
        };
        let repeat_expanded_node = Rc::new(RepeatExpandedNode {
            stack_frame: rtc.runtime.borrow_mut().peek_stack_frame().unwrap(),
            tab: repeat_expanded_node_tab.clone(),
            id_chain: id_chain.clone(),
            instance_node: Rc::clone(&node),
            parent_repeat_expanded_node: rtc.parent_repeat_expanded_node.clone(),
            node_context: rtc.distill_userland_node_context(),
        });

        //Note: ray-casting requires that the repeat_expanded_node_cache is sorted by z-index,
        //so the order in which `add_to_repeat_expanded_node_cache` is invoked vs. descendants is important
        (*rtc.engine.instance_registry).borrow_mut().add_to_repeat_expanded_node_cache(Rc::clone(&repeat_expanded_node));


        let instance_id = node.borrow().get_instance_id();

        //Determine if this node is marked for unmounting — either this has been passed as a flag from an ancestor that
        //was marked for deletion, or this instance_node is present in the InstanceRegistry's "marked for unmount" set.
        let marked_for_unmount = marked_for_unmount || self.instance_registry.borrow().marked_for_unmount_set.contains(&instance_id);


        //keep recursing through children
        children.borrow_mut().iter().rev().for_each(|child| {
            //note that we're iterating starting from the last child, for z-index (.rev())
            let mut new_rtc = rtc.clone();
            new_rtc.parent_repeat_expanded_node = Some(Rc::clone(&repeat_expanded_node));
            &self.recurse_traverse_render_tree(&mut new_rtc, rcs, Rc::clone(child), layer_info, marked_for_unmount );
            //FUTURE: for dependency management, return computed values from subtree above
        });



        let node_type = node.borrow_mut().get_layer_type();
        layer_info.update_depth(node_type);
        let current_depth = layer_info.get_depth();



        let is_viewport_culled = !repeat_expanded_node_tab.intersects(&self.viewport_tab);

        let last_layer = &rcs.len() -1;
        if let Some(rc) =  rcs.get_mut(current_depth) {
            //lifecycle: compute_native_patches — for elements with native components (for example Text, Frame, and form control elements),
            //certain native-bridge events must be triggered when changes occur, and some of those events require pre-computed `size` and `transform`.
            node.borrow_mut().compute_native_patches(rtc, new_accumulated_bounds, new_accumulated_transform.as_coeffs().to_vec(), current_depth);
            //lifecycle: render
            //this is this node's time to do its own rendering, aside
            //from the rendering of its children. Its children have already been rendered.
            if !is_viewport_culled {
                node.borrow_mut().handle_render(rtc, rc);
            }
        } else {
            node.borrow_mut().compute_native_patches(rtc, new_accumulated_bounds, new_accumulated_transform.as_coeffs().to_vec(), last_layer);
            if !is_viewport_culled {
                node.borrow_mut().handle_render(rtc, rcs.get_mut(last_layer).unwrap());
            }
        }


        //Handle node unmounting
        if marked_for_unmount {

            //lifecycle: will_unmount
            node.borrow_mut().handle_will_unmount(rtc);
            let id_chain = rtc.get_id_chain(instance_id);
            self.instance_registry.borrow_mut().mounted_set.remove(&id_chain);//, "Tried to unmount a node, but it was not mounted");
        }

        //lifecycle: did_render
        node.borrow_mut().handle_did_render(rtc, rcs);

    }

    /// Simple 2D raycasting: the coordinates of the ray represent a
    /// ray running orthogonally to the view plane, intersecting at
    /// the specified point `ray`.  Areas outside of clipping bounds will
    /// not register a `hit`, nor will elements that suppress input events.
    pub fn get_topmost_element_beneath_ray(&self, ray: (f64, f64)) -> Option<Rc<RepeatExpandedNode<R>>> {
        //Traverse all elements in render tree sorted by z-index (highest-to-lowest)
        //First: check whether events are suppressed
        //Next: check whether ancestral clipping bounds (hit_test) are satisfied
        //Finally: check whether element itself satisfies hit_test(ray)

        //Instead of storing a pointer to `last_rtc`, we should store a custom
        //struct with exactly the fields we need for ray-casting

        //Need:
        // - Cached computed transform `: Affine`
        // - Pointer to parent:
        //     for bubbling, i.e. propagating event
        //     for finding ancestral clipping containers
        //

        // reverse nodes to get top-most first (rendered in reverse order)
        let mut nodes_ordered : Vec<Rc<RepeatExpandedNode<R>>> = (*self.instance_registry).borrow()
            .repeat_expanded_node_cache.iter().rev()
            .map(|rc|{
                Rc::clone(rc)
            }).collect();

        // remove root element that is moved to top during reversal
        nodes_ordered.remove(0);

        // let ray = Point {x: ray.0,y: ray.1};
        let mut ret : Option<Rc<RepeatExpandedNode<R>>> = None;
        for node in nodes_ordered {
            // pax_runtime_api::log(&(**node).borrow().get_instance_id().to_string())


            if (*node.instance_node).borrow().ray_cast_test(&ray, &node.tab) {

                //We only care about the topmost node getting hit, and the element
                //pool is ordered by z-index so we can just resolve the whole
                //calculation when we find the first matching node

                let mut ancestral_clipping_bounds_are_satisfied = true;
                let mut parent : Option<Rc<RepeatExpandedNode<R>>> = node.parent_repeat_expanded_node.clone();

                loop {
                    if let Some(unwrapped_parent) = parent {
                        if (*unwrapped_parent.instance_node).borrow().is_clipping() && !(*unwrapped_parent.instance_node).borrow().ray_cast_test(&ray, &unwrapped_parent.tab) {
                            ancestral_clipping_bounds_are_satisfied = false;
                            break;
                        }
                        parent = unwrapped_parent.parent_repeat_expanded_node.clone();
                    } else {
                        break;
                    }
                }

                if ancestral_clipping_bounds_are_satisfied {
                    ret = Some(Rc::clone(&node));
                    break;
                }
            }
        }

        ret
    }

    pub fn get_focused_element(&self) -> Option<Rc<RepeatExpandedNode<R>>> {
        let (x, y) = self.viewport_tab.bounds;
        self.get_topmost_element_beneath_ray((x/2.0,y/2.0))
    }


    /// Called by chassis when viewport size changes, e.g. with native window resizes
    pub fn set_viewport_size(&mut self, new_viewport_size: (f64, f64)) {
        self.viewport_tab.bounds = new_viewport_size;
    }

    /// Workhorse method to advance rendering and property calculation by one discrete tick
    /// Will be executed synchronously up to 240 times/second.
    pub fn tick(&mut self, rcs: &mut Vec<R>) -> Vec<NativeMessage> {
        (*self.instance_registry).borrow_mut().reset_repeat_expanded_node_cache();
        let native_render_queue = self.traverse_render_tree(rcs);
        self.frames_elapsed = self.frames_elapsed + 1;
        native_render_queue
    }

    pub fn loadImage(&mut self, id_chain: Vec<u64>, image_data: Vec<u8>, width: usize, height: usize) {
        self.image_map.insert(id_chain, (Box::new(image_data), width, height));
    }
}
