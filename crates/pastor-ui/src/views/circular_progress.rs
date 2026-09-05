use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;
use gtk4::prelude::*;

#[derive(Clone)]
pub struct CircularProgress {
    drawing_area: gtk4::DrawingArea,
    fraction: Rc<RefCell<f64>>,
}

impl CircularProgress {
    pub fn new(size: i32) -> Self {
        let drawing_area = gtk4::DrawingArea::builder()
            .content_width(size)
            .content_height(size)
            .valign(gtk4::Align::Center)
            .halign(gtk4::Align::Center)
            .build();

        let fraction: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
        let fraction_clone = fraction.clone();

        drawing_area.set_draw_func(move |_, cr, width, height| {
            let frac: f64 = *fraction_clone.borrow();
            let w = width as f64;
            let h = height as f64;
            let center_x = w / 2.0;
            let center_y = h / 2.0;
            let line_width = 3.5;
            let radius = (center_x.min(center_y) - line_width).max(1.0);

            cr.set_line_width(line_width);

            // 1. Draw background track circle
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.25);
            let _ = cr.arc(center_x, center_y, radius, 0.0, 2.0 * PI);
            let _ = cr.stroke();

            // 2. Draw active progress arc (Parch primary blue #3584e4)
            if frac > 0.001 {
                cr.set_source_rgba(0.208, 0.518, 0.894, 1.0);
                let start_angle = -PI / 2.0;
                let end_angle = start_angle + (frac.clamp(0.0, 1.0) * 2.0 * PI);
                let _ = cr.arc(center_x, center_y, radius, start_angle, end_angle);
                let _ = cr.stroke();
            }
        });

        Self {
            drawing_area,
            fraction,
        }
    }

    pub fn widget(&self) -> &gtk4::DrawingArea {
        &self.drawing_area
    }

    pub fn set_fraction(&self, frac: f64) {
        *self.fraction.borrow_mut() = frac.clamp(0.0, 1.0);
        self.drawing_area.queue_draw();
    }
}
