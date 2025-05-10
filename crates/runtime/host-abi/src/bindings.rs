#[link(wasm_import_module = "icn_host")]
extern "C" {
    /// 0 = authorized, negative = policy failure
    pub fn host_check_resource_authorization(resource_type: u32, amount: u64) -> i32;
    /// 0 = recorded, negative = ledger/error
    pub fn host_record_resource_usage(resource_type: u32, amount: u64) -> i32;
    /// 0 = not in governance context, 1 = in governance context
    pub fn host_is_governance_context() -> i32;
    /// 0 = success, negative = error (only valid in governance context)
    pub fn host_mint_token(recipient_ptr: i32, recipient_len: i32, amount: u64) -> i32;
} 