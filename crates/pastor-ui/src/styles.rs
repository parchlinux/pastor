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
    padding: 4px 12px;
    font-weight: 600;
    font-size: 0.82rem;
    min-height: 34px;
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

/* Narrow-window compact mode (transaction bar, mobile layout, cards) (applied via Breakpoint);
   button hiding is done programmatically via TransactionBar::set_narrow */
.pastor-narrow .category-chip {
    padding: 4px 8px;
    font-size: 0.78rem;
    min-height: 34px;
}

.pastor-narrow .cascade-card {
    padding: 10px 12px;
    margin: 4px 0px;
}

.pastor-narrow .cascade-title {
    font-size: 1.15rem;
}

.pastor-narrow .cascade-pitch {
    font-size: 0.82rem;
    margin-bottom: 6px;
}

.pastor-narrow .cascade-banner-art {
    min-height: 110px;
}

.pastor-narrow .cascade-footer {
    padding: 6px 10px;
}

.pastor-narrow .cascade-get-btn {
    min-height: 44px;
    min-width: 44px;
    padding: 6px 14px;
}

.pastor-narrow headerbar {
    padding-left: 2px;
    padding-right: 2px;
}

.pastor-narrow headerbar button {
    min-width: 32px;
    min-height: 32px;
    padding: 2px 4px;
}

.pastor-narrow windowtitle .title {
    font-size: 0.95rem;
}

.pastor-narrow windowtitle .subtitle {
    font-size: 0.75rem;
}

.pastor-narrow .details-hero-box {
    margin-top: 4px;
    margin-bottom: 6px;
}

.details-primary-action-btn {
    min-height: 44px;
    padding: 8px 24px;
    font-weight: 600;
}

.pastor-narrow .details-primary-action-btn {
    min-height: 44px;
    padding: 8px 20px;
}

.pastor-narrow .app-icon-hero {
    border-radius: 18px;
    padding: 4px;
}

.pastor-narrow .screenshot-preview-card {
    min-height: 180px;
}

.pastor-narrow .screenshot-real-card {
    min-height: 180px;
}

.pastor-narrow .card {
    margin-left: 2px;
    margin-right: 2px;
}

.pastor-narrow .linked button {
    min-height: 36px;
    padding: 4px 8px;
    font-size: 0.8rem;
}

.pastor-narrow .pill {
    min-height: 44px;
}

.pastor-narrow .package-row-main {
    margin-left: 4px;
    margin-right: 4px;
}

.pastor-narrow .package-row-version {
    font-size: 0px;
    margin: 0;
    padding: 0;
}

.pastor-narrow .package-row-action-btn {
    min-width: 44px;
    min-height: 44px;
    padding: 4px;
    border-radius: 9999px;
}

.pastor-narrow .package-row-action-btn label {
    font-size: 0px;
    margin: 0;
    padding: 0;
}

/* Grid View Package Card (Square Tile) */
.package-grid-card {
    padding: 10px 8px;
    transition: all 180ms ease;
    min-width: 130px;
    min-height: 154px;
    margin: 3px;
    border-radius: 14px;
}

.package-grid-card:hover {
    border-color: alpha(@accent_color, 0.45);
    box-shadow: 0 4px 16px alpha(#000000, 0.12);
}

.package-grid-icon {
    border-radius: 12px;
    padding: 1px;
}

.package-grid-title {
    font-size: 0.84rem;
    font-weight: 700;
}

.package-grid-badge {
    font-size: 0.70rem;
    padding: 1px 6px;
}

.success-badge {
    background-color: alpha(#2ec27e, 0.18);
    color: #2ec27e;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 9999px;
    font-size: 0.76rem;
}

.destructive-badge {
    background-color: alpha(#e01b24, 0.18);
    color: #e01b24;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 9999px;
    font-size: 0.76rem;
}

.package-grid-action-btn,
.pastor-narrow .package-grid-action-btn {
    min-height: 28px;
    min-width: 68px;
    padding: 2px 8px;
    font-size: 0.76rem;
    border-radius: 9999px;
}

.package-grid-action-btn label,
.pastor-narrow .package-grid-action-btn label {
    font-size: 0.76rem;
    font-weight: 600;
    margin: 0;
    padding: 0;
}

gridview > child {
    padding: 2px;
    background: transparent;
    border: none;
    box-shadow: none;
}

gridview > child:hover,
gridview > child:selected {
    background: transparent;
}

.sidebar-row-title {
    font-size: 0.9rem;
}

.pastor-narrow .sidebar-row-title {
    font-size: 0.85rem;
}

flowboxchild {
    padding: 0;
    margin: 0;
    background: none;
}

/* Category-tinted placeholder screenshot art (DET-003) */
.cat-art-development { background-image: linear-gradient(160deg, alpha(@blue_3, 0.35), alpha(@purple_3, 0.25)); }
.cat-art-games       { background-image: linear-gradient(160deg, alpha(@orange_3, 0.35), alpha(@red_3, 0.25)); }
.cat-art-graphics    { background-image: linear-gradient(160deg, alpha(@purple_3, 0.35), alpha(@pink_3, 0.25)); }
.cat-art-internet    { background-image: linear-gradient(160deg, alpha(@teal_3, 0.35), alpha(@blue_3, 0.25)); }
.cat-art-multimedia  { background-image: linear-gradient(160deg, alpha(@purple_3, 0.35), alpha(@blue_3, 0.25)); }
.cat-art-office      { background-image: linear-gradient(160deg, alpha(@green_3, 0.35), alpha(@teal_3, 0.25)); }
.cat-art-system      { background-image: linear-gradient(160deg, alpha(@slate_3, 0.35), alpha(@window_fg_color, 0.12)); }
.cat-art-android     { background-image: linear-gradient(160deg, alpha(@green_4, 0.35), alpha(@green_3, 0.25)); }
.cat-art-utilities   { background-image: linear-gradient(160deg, alpha(@accent_color, 0.30), alpha(@accent_color, 0.10)); }

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

.details-context-bar {
    border-radius: 16px;
    padding: 6px 12px;
    margin: 4px 0px;
    background-color: alpha(@card_bg_color, 0.7);
    border: 1px solid alpha(@borders, 0.5);
}

.context-stat-item {
    padding: 6px 18px;
    min-width: 100px;
}

.context-stat-val {
    font-size: 0.92rem;
    font-weight: 700;
}

.pastor-narrow .details-context-bar {
    padding: 4px 6px;
}

.pastor-narrow .context-stat-item {
    padding: 6px 12px;
    min-width: 82px;
}

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

/* Doctor View Responsive Mobile Styling */
.doctor-header-title {
    font-size: 1.4rem;
    font-weight: 800;
}

.doctor-header-subtitle {
    font-size: 0.82rem;
}

.doctor-pacnew-actions {
    margin-left: 4px;
}

.doctor-action-btn {
    padding: 5px 10px;
    font-size: 0.82rem;
}

.pastor-narrow .doctor-header-title {
    font-size: 1.15rem;
}

.pastor-narrow .doctor-header-subtitle {
    font-size: 0.76rem;
}

.pastor-narrow .doctor-action-btn {
    padding: 3px 6px;
    font-size: 0.74rem;
    min-height: 28px;
}

.pastor-narrow .doctor-action-btn label {
    font-size: 0.74rem;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_css_parses_cleanly() {
        if !gtk4::is_initialized_main_thread() {
            if gtk4::is_initialized() || gtk4::init().is_err() {
                return;
            }
        }
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(APP_CSS);
    }
}
