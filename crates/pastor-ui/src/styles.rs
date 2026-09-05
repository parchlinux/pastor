pub const APP_CSS: &str = r#"
/* Parch Store Custom Styling */

.parch-badge-world {
    background-color: alpha(#3584e4, 0.2);
    color: #3584e4;
    border: 1px solid alpha(#3584e4, 0.4);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-void {
    background-color: alpha(#e66100, 0.2);
    color: #e66100;
    border: 1px solid alpha(#e66100, 0.4);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-arch {
    background-color: alpha(#1793d1, 0.15);
    color: #1793d1;
    border: 1px solid alpha(#1793d1, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-aur {
    background-color: alpha(#9141ac, 0.18);
    color: #9141ac;
    border: 1px solid alpha(#9141ac, 0.35);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-flatpak {
    background-color: alpha(#4a90d9, 0.15);
    color: #4a90d9;
    border: 1px solid alpha(#4a90d9, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-bootc {
    background-color: alpha(#2ec27e, 0.18);
    color: #2ec27e;
    border: 1px solid alpha(#2ec27e, 0.35);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-waydroid {
    background-color: alpha(#33d17a, 0.15);
    color: #26a269;
    border: 1px solid alpha(#26a269, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-update {
    background-color: alpha(#e66100, 0.18);
    color: #e66100;
    border: 1px solid alpha(#e66100, 0.4);
    border-radius: 9999px;
    padding: 2px 9px;
    font-weight: 600;
    font-size: 0.78rem;
}

.hero-banner {
    background: linear-gradient(135deg, #1c71d8 0%, #613583 100%);
    color: white;
    border-radius: 12px;
    padding: 24px;
    margin: 12px 18px;
}

.featured-card {
    background-color: alpha(@card_bg_color, 0.8);
    border-radius: 10px;
    padding: 12px;
    border: 1px solid alpha(@borders, 0.6);
    transition: all 180ms ease;
}

.featured-card:hover {
    background-color: @card_bg_color;
    border-color: @accent_color;
}

.console-box {
    font-family: monospace;
    font-size: 0.85rem;
    background-color: alpha(#000000, 0.4);
    border-radius: 8px;
    padding: 10px;
    color: #78aeed;
}

.transaction-pill {
    background-color: alpha(@accent_color, 0.15);
    border: 1px solid alpha(@accent_color, 0.4);
    border-radius: 20px;
    padding: 4px 12px;
}

/* GNOME HIG Application Details Page Styling */

.app-icon-hero {
    border-radius: 22px;
    box-shadow: 0 6px 20px alpha(#000000, 0.18);
    padding: 4px;
}

.metadata-badge {
    background-color: alpha(@card_bg_color, 0.9);
    border: 1px solid alpha(@borders, 0.6);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 500;
    font-size: 0.82rem;
}

.metadata-badge-accent {
    background-color: alpha(@accent_color, 0.12);
    border: 1px solid alpha(@accent_color, 0.35);
    border-radius: 9999px;
    padding: 3px 10px;
    color: @accent_color;
    font-weight: 500;
    font-size: 0.82rem;
}

.metadata-dot {
    color: alpha(@window_fg_color, 0.35);
    font-size: 0.85rem;
}

.screenshot-preview-card {
    background-color: alpha(@card_bg_color, 0.9);
    border: 1px solid alpha(@borders, 0.6);
    border-radius: 14px;
    box-shadow: 0 6px 20px alpha(#000000, 0.14);
    min-height: 290px;
    min-width: 520px;
}

.screenshot-real-card {
    border-radius: 14px;
    border: 1px solid alpha(@borders, 0.6);
    box-shadow: 0 6px 20px alpha(#000000, 0.14);
    background-color: alpha(@card_bg_color, 0.5);
}

.screenshot-preview-header {
    background-color: alpha(@window_bg_color, 0.75);
    border-bottom: 1px solid alpha(@borders, 0.35);
    border-top-left-radius: 14px;
    border-top-right-radius: 14px;
    padding: 8px 16px;
}

.screenshot-preview-body {
    padding: 20px;
}

.mock-sidebar-box {
    background-color: alpha(@window_bg_color, 0.4);
    border-radius: 8px;
    padding: 12px;
    min-width: 130px;
}

.mock-sidebar-bar {
    min-height: 8px;
    border-radius: 4px;
    background-color: alpha(@borders, 0.6);
    margin-bottom: 10px;
}

.mock-content-panel {
    background-color: alpha(@card_bg_color, 0.6);
    border: 1px solid alpha(@borders, 0.3);
    border-radius: 8px;
    padding: 14px;
}

.mock-data-bar {
    min-height: 8px;
    border-radius: 4px;
    background-color: alpha(@accent_color, 0.35);
    margin-bottom: 8px;
}

.dependency-tag {
    background-color: alpha(@card_bg_color, 0.9);
    border: 1px solid alpha(@borders, 0.5);
    border-radius: 8px;
    padding: 4px 10px;
    font-size: 0.82rem;
    font-family: monospace;
}

/* Apple Mac App Store Cascade Spotlight Styling */

.cascade-card {
    background: linear-gradient(180deg, alpha(@card_bg_color, 0.95) 0%, alpha(@card_bg_color, 0.75) 100%);
    border: 1px solid alpha(@borders, 0.7);
    border-radius: 16px;
    box-shadow: 0 6px 20px alpha(#000000, 0.12);
    padding: 14px 18px;
    transition: all 200ms cubic-bezier(0.25, 0.8, 0.25, 1);
    margin: 4px 8px;
}

.cascade-card:hover {
    box-shadow: 0 10px 28px alpha(#000000, 0.18);
    border-color: alpha(@accent_color, 0.5);
}

.cascade-eyebrow {
    font-size: 0.75rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: @accent_color;
    margin-bottom: 2px;
}

.cascade-title {
    font-size: 1.4rem;
    font-weight: 800;
    letter-spacing: -0.02em;
    color: @window_fg_color;
    margin-bottom: 2px;
}

.cascade-pitch {
    font-size: 0.88rem;
    color: alpha(@window_fg_color, 0.75);
    margin-bottom: 10px;
    line-height: 1.3;
}

.cascade-banner-art {
    border-radius: 12px;
    min-height: 140px;
    background: linear-gradient(135deg, alpha(@accent_color, 0.25) 0%, alpha(@accent_color, 0.05) 100%);
    border: 1px solid alpha(@borders, 0.4);
    margin-bottom: 10px;
}

.cascade-footer {
    background-color: alpha(@window_bg_color, 0.6);
    border-radius: 12px;
    padding: 8px 14px;
    border: 1px solid alpha(@borders, 0.35);
}

.cascade-app-title {
    font-size: 1.05rem;
    font-weight: 700;
    color: @window_fg_color;
}

.cascade-app-subtitle {
    font-size: 0.82rem;
    color: alpha(@window_fg_color, 0.65);
}

.cascade-get-btn {
    font-weight: 700;
    font-size: 0.85rem;
    padding: 6px 18px;
    border-radius: 9999px;
}

.active-install-card {
    background: linear-gradient(135deg, alpha(@accent_color, 0.12) 0%, alpha(@accent_color, 0.04) 100%);
    border: 1px solid alpha(@accent_color, 0.35);
    border-radius: 12px;
    padding: 12px 16px;
    margin-bottom: 8px;
}
"#;
