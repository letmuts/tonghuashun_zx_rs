/// 同花顺 API 交互中可能出现的错误类型。
use thiserror::Error;

/// 基础错误类型，所有同花顺相关错误的根。
#[derive(Error, Debug)]
pub enum ThsError {
    /// 远程 API 返回的业务错误。
    #[error("{action} 失败: {message}{}", .code.as_deref().map_or(String::new(), |c| format!(" (code={})", c)))]
    Api {
        action: String,
        message: String,
        code: Option<String>,
    },

    /// 网络层面的错误。
    #[error("{action} 失败: {message}")]
    Network { action: String, message: String },

    /// 认证相关错误。
    #[error("认证失败: {0}")]
    Auth(String),
}

/// 创建 API 错误的便捷函数。
pub fn api_error(action: impl Into<String>, message: impl Into<String>) -> ThsError {
    ThsError::Api {
        action: action.into(),
        message: message.into(),
        code: None,
    }
}

/// 创建带错误码的 API 错误。
pub fn api_error_with_code(
    action: impl Into<String>,
    message: impl Into<String>,
    code: impl Into<String>,
) -> ThsError {
    ThsError::Api {
        action: action.into(),
        message: message.into(),
        code: Some(code.into()),
    }
}

/// 创建网络错误的便捷函数。
pub fn network_error(action: impl Into<String>, message: impl Into<String>) -> ThsError {
    ThsError::Network {
        action: action.into(),
        message: message.into(),
    }
}

/// 创建认证错误的便捷函数。
pub fn auth_error(message: impl Into<String>) -> ThsError {
    ThsError::Auth(message.into())
}

/// 自定义 Result 别名，简化错误传播。
pub type ThsResult<T> = Result<T, ThsError>;
