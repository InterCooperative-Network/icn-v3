use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ResourceType {
    Cpu    = 1,
    Memory = 2,
    Io     = 3,
    Token  = 4,
}

impl From<u32> for ResourceType {
    fn from(v: u32) -> Self {
        match v {
            1 => ResourceType::Cpu,
            2 => ResourceType::Memory,
            3 => ResourceType::Io,
            _ => ResourceType::Token,
        }
    }
} 