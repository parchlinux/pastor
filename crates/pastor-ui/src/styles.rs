pub const APP_CSS: &str = r#"
/* Parch Store Custom Styling */

.parch-badge-world {
    background-color: alpha(@blue_3, 0.2);
    color: @blue_3;
    border: 1px solid alpha(@blue_3, 0.4);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-void {
    background-color: alpha(@orange_3, 0.2);
    color: @orange_3;
    border: 1px solid alpha(@orange_3, 0.4);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-arch {
    background-color: alpha(@blue_4, 0.15);
    color: @blue_4;
    border: 1px solid alpha(@blue_4, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-aur {
    background-color: alpha(@purple_3, 0.18);
    color: @purple_3;
    border: 1px solid alpha(@purple_3, 0.35);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-flatpak {
    background-color: alpha(@accent_color, 0.15);
    color: @accent_color;
    border: 1px solid alpha(@accent_color, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-bootc {
    background-color: alpha(@green_3, 0.18);
    color: @green_3;
    border: 1px solid alpha(@green_3, 0.35);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: bold;
    font-size: 0.82rem;
}

.parch-badge-waydroid {
    background-color: alpha(@green_4, 0.15);
    color: @green_4;
    border: 1px solid alpha(@green_4, 0.3);
    border-radius: 9999px;
    padding: 3px 10px;
    font-weight: 600;
    font-size: 0.82rem;
}

.parch-badge-update {
    background-color: alpha(@orange_3, 0.18);
    color: @orange_3;
    border: 1px solid alpha(@orange_3, 0.4);
    border-radius: 9999px;
    padding: 2px 9px;
    font-weight: 600;
    font-size: 0.78rem;
}

.hero-banner {
    background: linear-gradient(135deg, alpha(@accent_color, 0.85) 0%, alpha(@purple_3, 0.85) 100%);
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
    background-color: alpha(@window_bg_color, 0.85);
    border: 1px solid alpha(@borders, 0.5);
    border-radius: 8px;
    padding: 10px;
    color: @accent_color;
}

.transaction-pill {
    background-color: alpha(@accent_color, 0.15);
    border: 1px solid alpha(@accent_color, 0.4);
    border-radius: 20px;
    padding: 4px 12px;
}

/* Skeleton Loading (EXP-002) */
@keyframes skeleton-shimmer {
    0%   { background-position: -600px 0; }
    100% { background-position: 600px 0; }
}

.skeleton-card {
    background: linear-gradient(90deg,
        alpha(@card_bg_color, 0.5) 25%,
        alpha(@card_bg_color, 0.85) 50%,
        alpha(@card_bg_color, 0.5) 75%
    );
    background-size: 1200px 100%;
    animation: skeleton-shimmer 1.4s infinite linear;
    border-radius: 16px;
    min-height: 120px;
}

/* Transaction Bar States (TRX-002) */
.transaction-bar-active {
    border-left: 4px solid @accent_color;
}

.transaction-bar-success {
    border-left: 4px solid @success_color;
}

.transaction-bar-failed {
    border-left: 4px solid @error_color;
}

/* Minimum Touch Targets & Chip (VIS-006, EXP-001) */
.category-chip {
    border-radius: 9999px;
    padding: 6px 14px;
    font-weight: 600;
    font-size: 0.85rem;
    min-height: 38px;
}

button.dependency-tag {
    background-color: alpha(@card_bg_color, 0.9);
    border: 1px solid alpha(@borders, 0.5);
    border-radius: 8px;
    padding: 4px 10px;
    font-size: 0.82rem;
    font-family: monospace;
    min-height: 36px;
    min-width: 44px;
}

button.dependency-tag:hover {
    background-color: alpha(@accent_color, 0.15);
    border-color: alpha(@accent_color, 0.4);
    color: @accent_color;
}

/* Hero Screenshot Overlay (EXP-004) */
.hero-screenshot-overlay {
    border-radius: 16px;
    background: linear-gradient(
        to bottom,
        alpha(#000000, 0.05) 0%,
        alpha(#000000, 0.65) 100%
    );
}

/* GNOME HIG Application Details Page Styling */

.app-icon-hero {
    border-radius: 22px;
    box-shadow: 0 6px 20px alpha(#000000, 0.18);
    background-color: alpha(@window_bg_color, 0.3);
    padding: 6px;
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
    min-height: 260px;
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
