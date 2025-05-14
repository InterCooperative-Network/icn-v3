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
    /// 0 = success, -1 = insufficient funds, -2 = malformed DID
    pub fn host_transfer_token(
        sender_ptr: i32,
        sender_len: i32,
        recipient_ptr: i32,
        recipient_len: i32,
        amount: u64,
    ) -> i32;
    /// Anchor a serialized ExecutionReceipt into the DAG.
    /// ptr/len: receipt bytes; returns 0 on success.
    pub fn host_anchor_receipt(ptr: u32, len: u32) -> i32;
    /// Submit a job to the mesh network.
    /// job_params_cbor_ptr/len: Serialized MeshJobParams (CBOR).
    /// job_id_buffer_ptr/len: Pointer and length of a buffer in WASM memory where the host will write the JobId string.
    /// Returns: Actual length of the JobId string written to the buffer if successful (can be less than buffer_len).
    ///          Returns 0 if job_id_buffer_len is too small to write even a minimal JobId (host should define minimum).
    ///          Returns a negative error code on failure (e.g., deserialization error, queueing error).
    pub fn host_submit_mesh_job(
        job_params_cbor_ptr: i32,
        job_params_cbor_len: i32,
        job_id_buffer_ptr: i32,
        job_id_buffer_len: i32,
    ) -> i32;
}

pub const ICN_HOST_ABI_VERSION: u32 = 8;
