-- Disable foreign key constraints temporarily
PRAGMA foreign_keys=off;

BEGIN TRANSACTION;

-- Rename the current 'jobs' table (which has the new schema)
ALTER TABLE jobs RENAME TO jobs_new_temp;

-- Recreate the 'jobs' table with the approximate old schema
CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY,
    request_json TEXT, -- Will attempt to reconstruct
    status_type TEXT NOT NULL,
    status_did TEXT,
    status_reason TEXT,
    deadline INTEGER,
    wasm_cid TEXT, -- Will attempt to extract from params_json
    description TEXT, -- Will attempt to extract from params_json
    winning_bid_id TEXT, -- Assuming this column existed or was used
    updated_at TIMESTAMP
    -- created_at is omitted as it was added in the 'up' script with a default
    -- If the old table truly had a created_at, it should be added here.
);

-- Migrate data from jobs_new_temp back to the recreated jobs table
-- This is an approximation, especially for request_json.
-- It assumes wasm_cid and description are top-level fields in params_json.
-- The old JobRequest also had 'requirements' and 'metadata' which are harder to reconstruct.
INSERT INTO jobs (
    job_id,
    request_json, -- Approximated
    status_type,
    status_did,
    status_reason,
    deadline,
    wasm_cid,
    description,
    winning_bid_id,
    updated_at
)
SELECT
    job_id,
    -- Attempt to reconstruct a semblance of the old request_json.
    -- This will be a new JSON object. The old JobRequest structure was specific.
    -- This example puts params and originator_did into a new JSON.
    -- A more accurate reconstruction would depend on the exact old JobRequest fields
    -- and whether they can all be found within the new params_json.
    json_object(
        'params', json(params_json), -- Embed the whole MeshJobParams
        'originator_did', originator_did,
        'wasm_cid', json_extract(params_json, '$.wasm_cid'), -- For the old JobRequest.wasm_cid field (as Cid string)
        'description', json_extract(params_json, '$.description'), -- For old JobRequest.description
        'deadline', json_extract(params_json, '$.deadline') -- For old JobRequest.deadline (as u64 timestamp)
        -- 'requirements' and 'metadata' from old JobRequest are not easily reconstructed here.
    ),
    status_type,
    status_did,
    status_reason,
    json_extract(params_json, '$.deadline'), -- deadline was also in MeshJobParams
    json_extract(params_json, '$.wasm_cid'), -- Populate separate wasm_cid column
    json_extract(params_json, '$.description'), -- Populate separate description column
    winning_bid_id,
    updated_at
FROM jobs_new_temp;

-- Drop the temporary table
DROP TABLE jobs_new_temp;

COMMIT;

-- Re-enable foreign key constraints
PRAGMA foreign_keys=on; 