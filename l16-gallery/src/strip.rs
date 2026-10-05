// The viewer's photos side by side, as a phone gallery's: the previous, the shown and the next
// photo in a row (a small gap between them), moved together by an offset that follows the
// finger, then slid the rest of the way (or back) by animate_to.

use gtk::glib;
use gtk::graphene;
use gtk::gsk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

// between neighbouring photos while they slide
pub const GAP: f64 = 24.0;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    pub struct Anim {
        pub from: f64,
        pub to: f64,
        pub start: i64,
        pub length: i64,
        pub tick: Option<gtk::TickCallbackId>,
        pub done: Option<Box<dyn FnOnce()>>,
    }

    #[derive(Default)]
    pub struct Strip {
        pub children: RefCell<Vec<gtk::Widget>>,
        pub offset: Cell<f64>,
        pub anim: RefCell<Option<Anim>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Strip {
        const NAME: &'static str = "L16Strip";
        type Type = super::Strip;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Strip {
        fn dispose(&self) {
            for c in self.children.take() {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Strip {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let children = self.children.borrow();
            let Some(c) = children.get(1) else { return (0, 0, -1, -1) };
            let (min, nat, _, _) = c.measure(orientation, for_size);
            (min, nat, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let step = width as f64 + GAP;
            for (k, c) in self.children.borrow().iter().enumerate() {
                c.measure(gtk::Orientation::Horizontal, -1);
                let x = (k as f64 - 1.0) * step + self.offset.get();
                let t = gsk::Transform::new().translate(&graphene::Point::new(x as f32, 0.0));
                c.allocate(width, height, -1, Some(t));
            }
        }
    }
}

glib::wrapper! {
    pub struct Strip(ObjectSubclass<imp::Strip>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Strip {
    // the previous, shown and next photo's widgets, in that order
    pub fn new(children: [&gtk::Widget; 3]) -> Self {
        let s: Self = glib::Object::new();
        s.set_overflow(gtk::Overflow::Hidden);
        for c in children {
            c.set_parent(&s);
            s.imp().children.borrow_mut().push(c.clone());
        }
        s
    }

    // the distance from one photo to the next
    pub fn step(&self) -> f64 {
        self.width() as f64 + GAP
    }

    pub fn offset(&self) -> f64 {
        self.imp().offset.get()
    }

    pub fn set_offset(&self, x: f64) {
        self.imp().offset.set(x);
        self.queue_allocate();
    }

    pub fn animating(&self) -> bool {
        self.imp().anim.borrow().is_some()
    }

    // slide to @to (ease out, quicker for a shorter way), then @done
    pub fn animate_to(&self, to: f64, done: impl FnOnce() + 'static) {
        self.finish();
        let from = self.offset();
        let way = ((to - from).abs() / self.step().max(1.0)).min(1.0);
        let length = (60_000.0 + 200_000.0 * way) as i64; // us
        let start = self.frame_clock().map_or_else(glib::monotonic_time, |c| c.frame_time());
        let tick = self.add_tick_callback(|s, clock| {
            let (t, to) = {
                let anim = s.imp().anim.borrow();
                let Some(a) = anim.as_ref() else { return glib::ControlFlow::Break };
                (((clock.frame_time() - a.start) as f64 / a.length as f64).clamp(0.0, 1.0), a.to)
            };
            if t >= 1.0 {
                // the callback ends by returning Break: finish() mustn't remove it as well
                if let Some(a) = s.imp().anim.borrow_mut().as_mut() {
                    a.tick = None;
                }
                s.finish();
                return glib::ControlFlow::Break;
            }
            let from = s.imp().anim.borrow().as_ref().map_or(to, |a| a.from);
            let ease = 1.0 - (1.0 - t).powi(3);
            s.set_offset(from + (to - from) * ease);
            glib::ControlFlow::Continue
        });
        *self.imp().anim.borrow_mut() = Some(imp::Anim {
            from,
            to,
            start,
            length,
            tick: Some(tick),
            done: Some(Box::new(done)),
        });
    }

    // a running slide straight to its end (and its @done)
    pub fn finish(&self) {
        let Some(mut a) = self.imp().anim.borrow_mut().take() else { return };
        if let Some(tick) = a.tick.take() {
            tick.remove();
        }
        self.set_offset(a.to);
        if let Some(done) = a.done.take() {
            done();
        }
    }
}
