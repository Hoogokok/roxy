use crate::settings::typestate::TypeState;

/// HTTPS 활성화 상태를 표현하는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsEnabled;

/// HTTPS 비활성화 상태를 나타내는 타입
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsDisabled;

impl TypeState for HttpsDisabled {}
impl TypeState for HttpsEnabled {} 