#[link(wasm_import_module = "icn_host")]
extern "C" {
    /// 0 = authorized, negative = policy failure
    pub fn host_check_resource_authorization(resource_type: u32, amount: u64) -> i32;
    /// 0 = recorded, negative = ledger/error
    pub fn host_record_resource_usage(resource_type: u32, amount: u64) -> i32;
} 