// The preview: the camera's paintable, filling the widget, cropped in by a zoom factor
// (digital zoom between the modules' focal lengths). Its digital gain is the software ISP's.

use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct ZoomView {
        pub paintable: RefCell<Option<gdk::Paintable>>,
        pub zoom: Cell<f64>,
        pub next_zoom: Cell<Option<f64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ZoomView {
        const NAME: &'static str = "L16ZoomView";
        type Type = super::ZoomView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ZoomView {}

    impl WidgetImpl for ZoomView {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let (w, h) = (widget.width() as f64, widget.height() as f64);
            let Some(p) = self.paintable.borrow().clone() else { return };
            let ar = p.intrinsic_aspect_ratio();
            if ar <= 0.0 || w <= 0.0 || h <= 0.0 {
                return;
            }
            // cover the widget, then crop in
            let (mut pw, mut ph) = if w / h > ar { (w, w / ar) } else { (h * ar, h) };
            // libcamera's software ISP leaves its last column blue: push a source pixel off
            // each edge
            let src_w = p.intrinsic_width().max(3) as f64;
            let z = self.zoom.get().max(1.0) * src_w / (src_w - 2.0);
            pw *= z;
            ph *= z;
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
            snapshot.save();
            snapshot.translate(&graphene::Point::new(((w - pw) / 2.0) as f32, ((h - ph) / 2.0) as f32));
            p.snapshot(snapshot, pw, ph);
            snapshot.restore();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct ZoomView(ObjectSubclass<imp::ZoomView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ZoomView {
    pub fn new(paintable: &gdk::Paintable) -> Self {
        let view: ZoomView = glib::Object::new();
        view.imp().zoom.set(1.0);
        view.imp().paintable.replace(Some(paintable.clone()));
        let weak = view.downgrade();
        paintable.connect_invalidate_contents(move |_| {
            if let Some(v) = weak.upgrade() {
                if let Some(z) = v.imp().next_zoom.take() {
                    v.imp().zoom.set(z);
                }
                v.queue_draw();
            }
        });
        view
    }

    pub fn set_zoom(&self, zoom: f64) {
        self.imp().next_zoom.set(None);
        self.imp().zoom.set(zoom);
        self.queue_draw();
    }

    pub fn zoom(&self) -> f64 {
        self.imp().zoom.get().max(1.0)
    }

    // from the next frame on (the first of another module)
    pub fn set_zoom_next_frame(&self, zoom: f64) {
        self.imp().next_zoom.set(Some(zoom));
    }
}
