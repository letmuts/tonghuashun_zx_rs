/// 协议实现模块。
///
/// 包含同花顺自选股管理的四种底层协议实现：
/// - `blockstock`: multiStorage 协议，用于批量读写自定义分组
/// - `selfstock_v1`: v1 自选股协议，用于"我的自选"批量读写
/// - `selfstock_v2`: v2 自选股协议，用于"我的自选"单条读写
/// - `dynamicplate`: 动态板块查询协议

pub mod blockstock;
pub mod dynamicplate;
pub mod selfstock_v1;
pub mod selfstock_v2;
