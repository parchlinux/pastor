use adw::prelude::*;

pub fn get_cache_path(url: &str) -> std::path::PathBuf {
    let mut cache_dir = gtk4::glib::user_cache_dir();
    cache_dir.push("pastor");
    cache_dir.push("screenshots");
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(url, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);

    let ext = if url.ends_with(".png") {
        "png"
    } else if url.ends_with(".webp") {
        "webp"
    } else {
        "jpg"
    };

    cache_dir.push(format!("{:x}.{}", hash, ext));
    cache_dir
}

pub fn create_screenshot_widget(url: &str) -> gtk4::Widget {
    let overlay = gtk4::Overlay::builder()
        .height_request(320)
        .width_request(560)
        .css_classes(["screenshot-real-card"])
        .build();

    let picture = gtk4::Picture::builder()
        .keep_aspect_ratio(true)
        .can_shrink(true)
        .vexpand(true)
        .hexpand(true)
        .build();

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .width_request(36)
        .height_request(36)
        .build();

    let cache_file = get_cache_path(url);
    if cache_file.is_file() {
        picture.set_filename(Some(&cache_file));
        spinner.set_visible(false);
    } else {
        spinner.set_visible(true);
        let pic_clone = picture.clone();
        let spin_clone = spinner.clone();
        let url_string = url.to_string();
        let target_file = cache_file.clone();

        gtk4::glib::spawn_future_local(async move {
            let target_for_thread = target_file.clone();
            let res = tokio::task::spawn_blocking(move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let resp = ureq::get(&url_string).timeout(std::time::Duration::from_secs(8)).call()?;
                let mut reader = resp.into_reader();
                let tmp_file = target_for_thread.with_extension("tmp");
                let mut f = std::fs::File::create(&tmp_file)?;
                std::io::copy(&mut reader, &mut f)?;
                std::fs::rename(&tmp_file, &target_for_thread)?;
                Ok(())
            }).await;

            if let Ok(Ok(())) = res {
                pic_clone.set_filename(Some(&target_file));
            }
            spin_clone.set_visible(false);
        });
    }

    overlay.set_child(Some(&picture));
    overlay.add_overlay(&spinner);
    overlay.upcast()
}
