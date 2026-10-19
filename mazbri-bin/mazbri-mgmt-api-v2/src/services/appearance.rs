use crate::repositories::appearance as repo;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::info;
#[derive(Debug, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub system_theme_mode: String,
    pub system_theme_color: String,
    pub container_width: String,
    pub box_border_mode: bool,
    pub menu_open_width: i32,
    pub show_work_tab: bool,
    pub show_menu_button: bool,
    pub show_fast_enter: bool,
    pub show_refresh_button: bool,
    pub show_crumbs: bool,
    pub show_language: bool,
    pub show_nprogress: bool,
    pub watermark_visible: bool,
    pub tab_style: String,
    pub page_transition: String,
    pub custom_radius: String,
    pub dark_theme_style: String,
    pub updated_at: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct UpdateAppearanceRequest {
    pub system_theme_mode: Option<String>,
    pub system_theme_color: Option<String>,
    pub container_width: Option<String>,
    pub box_border_mode: Option<bool>,
    pub menu_open_width: Option<i32>,
    pub show_work_tab: Option<bool>,
    pub show_menu_button: Option<bool>,
    pub show_fast_enter: Option<bool>,
    pub show_refresh_button: Option<bool>,
    pub show_crumbs: Option<bool>,
    pub show_language: Option<bool>,
    pub show_nprogress: Option<bool>,
    pub watermark_visible: Option<bool>,
    pub tab_style: Option<String>,
    pub page_transition: Option<String>,
    pub custom_radius: Option<String>,
    pub dark_theme_style: Option<String>,
}
fn row_to_settings(row: &repo::AppearanceRow) -> AppearanceSettings {
    AppearanceSettings {
        system_theme_mode: row.system_theme_mode.clone(),
        system_theme_color: row.system_theme_color.clone(),
        container_width: row.container_width.clone(),
        box_border_mode: row.box_border_mode,
        menu_open_width: row.menu_open_width,
        show_work_tab: row.show_work_tab,
        show_menu_button: row.show_menu_button,
        show_fast_enter: row.show_fast_enter,
        show_refresh_button: row.show_refresh_button,
        show_crumbs: row.show_crumbs,
        show_language: row.show_language,
        show_nprogress: row.show_nprogress,
        watermark_visible: row.watermark_visible,
        tab_style: row.tab_style.clone(),
        page_transition: row.page_transition.clone(),
        custom_radius: row.custom_radius.clone(),
        dark_theme_style: row.dark_theme_style.clone(),
        updated_at: row.updated_at.map(|t| t.to_rfc3339()),
    }
}
pub fn validate_update(payload: &UpdateAppearanceRequest) -> Result<(), String> {
    if let Some(mode) = &payload.system_theme_mode {
        if !["auto", "light", "dark"].contains(&mode.as_str()) {
            return Err("无效的主题模式".to_string());
        }
    }
    if let Some(color) = &payload.system_theme_color {
        if !color.starts_with('#') || color.len() != 7 {
            return Err("无效的主题颜色格式（必须是 #RRGGBB）".to_string());
        }
    }
    if let Some(width) = &payload.container_width {
        if !["100%", "1200px"].contains(&width.as_str()) {
            return Err("无效的容器宽度".to_string());
        }
    }
    if let Some(width) = payload.menu_open_width {
        if !(180..=320).contains(&width) {
            return Err("菜单宽度必须在 180-320 之间".to_string());
        }
    }
    if let Some(style) = &payload.dark_theme_style {
        if !["obsidian", "graphite", "oceanic", "nordic"].contains(&style.as_str()) {
            return Err("无效的暗色主题风格".to_string());
        }
    }
    Ok(())
}
pub async fn get_settings(pool: &PgPool) -> Result<AppearanceSettings, String> {
    repo::fetch_settings(pool)
        .await
        .map(|row| row_to_settings(&row))
        .map_err(|e| {
            tracing::error!("Failed to get appearance settings: {}", e);
            "获取外观设置失败".to_string()
        })
}
pub async fn update_settings(
    pool: &PgPool,
    user_id: &str,
    admin_sub: &str,
    payload: &UpdateAppearanceRequest,
) -> Result<AppearanceSettings, String> {
    validate_update(payload)?;
    repo::update_settings(
        pool,
        repo::UpdateAppearanceParams {
            system_theme_mode: &payload.system_theme_mode,
            system_theme_color: &payload.system_theme_color,
            container_width: &payload.container_width,
            box_border_mode: &payload.box_border_mode,
            menu_open_width: &payload.menu_open_width,
            show_work_tab: &payload.show_work_tab,
            show_menu_button: &payload.show_menu_button,
            show_fast_enter: &payload.show_fast_enter,
            show_refresh_button: &payload.show_refresh_button,
            show_crumbs: &payload.show_crumbs,
            show_language: &payload.show_language,
            show_nprogress: &payload.show_nprogress,
            watermark_visible: &payload.watermark_visible,
            tab_style: &payload.tab_style,
            page_transition: &payload.page_transition,
            custom_radius: &payload.custom_radius,
            dark_theme_style: &payload.dark_theme_style,
            updated_by: user_id,
        },
    )
    .await
    .map(|row| {
        info!("Appearance settings updated by admin {}", admin_sub);
        row_to_settings(&row)
    })
    .map_err(|e| {
        tracing::error!("Failed to update appearance settings: {}", e);
        "更新外观设置失败".to_string()
    })
}
pub async fn reset_settings(
    pool: &PgPool,
    user_id: &str,
    admin_sub: &str,
) -> Result<AppearanceSettings, String> {
    repo::reset_settings(pool, user_id)
        .await
        .map(|row| {
            info!(
                "Appearance settings reset to defaults by admin {}",
                admin_sub
            );
            row_to_settings(&row)
        })
        .map_err(|e| {
            tracing::error!("Failed to reset appearance settings: {}", e);
            "重置外观设置失败".to_string()
        })
}
