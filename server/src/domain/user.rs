/// 账户在领域层的基础数据。
///
/// `password_hash` 只能保存经过安全哈希后的密码，不能保存明文密码。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub email: String,
}

impl User {
    pub fn new(
        id: i64,
        username: impl Into<String>,
        password_hash: impl Into<String>,
        email: impl Into<String>,
    ) -> Self {
        Self {
            id,
            username: username.into(),
            password_hash: password_hash.into(),
            email: email.into(),
        }
    }
}
