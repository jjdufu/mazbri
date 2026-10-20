use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
/// 导航区显示开关（工作标签、面包屑、语言切换）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::FromRow)]
pub struct NavigationToggles {
    pub show_work_tab: bool,
    pub show_crumbs: bool,
    pub show_language: bool,
}
/// 顶栏按钮显示开关（菜单、快速入口、刷新）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::FromRow)]
pub struct HeaderBarToggles {
    pub show_menu_button: bool,
    pub show_fast_enter: bool,
    pub show_refresh_button: bool,
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AppearanceRow {
    pub system_theme_mode: String,
    pub system_theme_color: String,
    pub container_width: String,
    pub box_border_mode: bool,
    pub menu_open_width: i32,
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub navigation: NavigationToggles,
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub header_bar: HeaderBarToggles,
    pub show_nprogress: bool,
    pub watermark_visible: bool,
    pub tab_style: String,
    pub page_transition: String,
    pub custom_radius: String,
    pub dark_theme_style: String,
    pub updated_at: Option<DateTime<Utc>>,
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn fetch_settings(pool: &PgPool) -> Result<AppearanceRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        AppearanceRow,
    >(
            r"
        SELECT system_theme_mode, system_theme_color, container_width, box_border_mode, menu_open_width,
            show_work_tab, show_menu_button, show_fast_enter, show_refresh_button,
            show_crumbs, show_language, show_nprogress, watermark_visible,
            tab_style, page_transition, custom_radius, dark_theme_style, updated_at
        FROM appearance_settings WHERE id = '00000000-0000-0000-0000-000000000100'
    ",
        )
        .fetch_one(pool)
        .await
}
pub struct UpdateAppearanceParams<'a> {
    pub system_theme_mode: &'a Option<String>,
    pub system_theme_color: &'a Option<String>,
    pub container_width: &'a Option<String>,
    pub box_border_mode: &'a Option<bool>,
    pub menu_open_width: &'a Option<i32>,
    pub show_work_tab: &'a Option<bool>,
    pub show_menu_button: &'a Option<bool>,
    pub show_fast_enter: &'a Option<bool>,
    pub show_refresh_button: &'a Option<bool>,
    pub show_crumbs: &'a Option<bool>,
    pub show_language: &'a Option<bool>,
    pub show_nprogress: &'a Option<bool>,
    pub watermark_visible: &'a Option<bool>,
    pub tab_style: &'a Option<String>,
    pub page_transition: &'a Option<String>,
    pub custom_radius: &'a Option<String>,
    pub dark_theme_style: &'a Option<String>,
    pub updated_by: &'a str,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn update_settings(
    pool: &PgPool,
    params: UpdateAppearanceParams<'_>,
) -> Result<AppearanceRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        AppearanceRow,
    >(
            r"
        UPDATE appearance_settings SET
            system_theme_mode = COALESCE($1, system_theme_mode), system_theme_color = COALESCE($2, system_theme_color),
            container_width = COALESCE($3, container_width), box_border_mode = COALESCE($4, box_border_mode),
            menu_open_width = COALESCE($5, menu_open_width), show_work_tab = COALESCE($6, show_work_tab),
            show_menu_button = COALESCE($7, show_menu_button), show_fast_enter = COALESCE($8, show_fast_enter),
            show_refresh_button = COALESCE($9, show_refresh_button), show_crumbs = COALESCE($10, show_crumbs),
            show_language = COALESCE($11, show_language), show_nprogress = COALESCE($12, show_nprogress),
            watermark_visible = COALESCE($13, watermark_visible), tab_style = COALESCE($14, tab_style),
            page_transition = COALESCE($15, page_transition), custom_radius = COALESCE($16, custom_radius),
            dark_theme_style = COALESCE($17, dark_theme_style),
            updated_at = NOW(), updated_by = $18::uuid
        WHERE id = '00000000-0000-0000-0000-000000000100'
        RETURNING system_theme_mode, system_theme_color, container_width, box_border_mode, menu_open_width,
            show_work_tab, show_menu_button, show_fast_enter, show_refresh_button,
            show_crumbs, show_language, show_nprogress, watermark_visible,
            tab_style, page_transition, custom_radius, dark_theme_style, updated_at
    ",
        )
        .bind(params.system_theme_mode)
        .bind(params.system_theme_color)
        .bind(params.container_width)
        .bind(params.box_border_mode)
        .bind(params.menu_open_width)
        .bind(params.show_work_tab)
        .bind(params.show_menu_button)
        .bind(params.show_fast_enter)
        .bind(params.show_refresh_button)
        .bind(params.show_crumbs)
        .bind(params.show_language)
        .bind(params.show_nprogress)
        .bind(params.watermark_visible)
        .bind(params.tab_style)
        .bind(params.page_transition)
        .bind(params.custom_radius)
        .bind(params.dark_theme_style)
        .bind(params.updated_by)
        .fetch_one(pool)
        .await
}
/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn reset_settings(pool: &PgPool, updated_by: &str) -> Result<AppearanceRow, sqlx::Error> {
    sqlx::query_as::<
        _,
        AppearanceRow,
    >(
            r"
        UPDATE appearance_settings SET
            system_theme_mode = 'auto', system_theme_color = '#5D87FF', container_width = '100%', box_border_mode = false,
            menu_open_width = 200, show_work_tab = false,
            show_menu_button = true, show_fast_enter = true, show_refresh_button = true,
            show_crumbs = true, show_language = true, show_nprogress = true,
            watermark_visible = false, tab_style = 'tab-default',
            page_transition = 'fade', custom_radius = '0', dark_theme_style = 'obsidian',
            updated_at = NOW(), updated_by = $1::uuid
        WHERE id = '00000000-0000-0000-0000-000000000100'
        RETURNING system_theme_mode, system_theme_color, container_width, box_border_mode, menu_open_width,
            show_work_tab, show_menu_button, show_fast_enter, show_refresh_button,
            show_crumbs, show_language, show_nprogress, watermark_visible,
            tab_style, page_transition, custom_radius, dark_theme_style, updated_at
    ",
        )
        .bind(updated_by)
        .fetch_one(pool)
        .await
}
