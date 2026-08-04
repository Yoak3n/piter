use super::schema::WindowType;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WindowConfig {
    // title should change with window type, so there is no need to set it in config
    pub window_type: WindowType,
    pub inner_size: (f64, f64),
    pub min_inner_size: (f64, f64),
    pub decorations: bool,
    pub transparent: bool,
    pub skip_taskbar: bool,
    pub shadow: bool,
    pub always_on_top: bool,
    pub maximizable: bool,
    pub focused: bool,
    pub center: bool,
    pub float: bool,
}

impl WindowConfig {
    pub fn new(window_type: WindowType) -> Self {
        match window_type {
            WindowType::Main => Self {
                window_type,
                inner_size: (1440.0, 960.0),
                min_inner_size: (400.0, 80.0),
                // 自定义标题栏：去掉系统标题栏，由前端 TitleBar 提供拖拽与
                // 窗口控制；Windows 下用 shadow 补偿无边框窗口的系统阴影。
                decorations: false,
                transparent: false,
                skip_taskbar: false,
                shadow: true,
                always_on_top: false,
                maximizable: true,
                focused: true,
                center: true,
                float: false,
            },
        }
    }
    pub fn default_config() -> HashMap<WindowType, WindowConfig> {
        HashMap::from([(
            WindowType::Main,
            WindowConfig::new(WindowType::Main),
        )])
    }
}