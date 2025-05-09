#!/bin/bash
set -e

echo "===== ICN Core Components Verification ====="
echo

# Move to the repository root
cd "$(git rev-parse --show-toplevel)"

echo "1. CID Parity Verification"
echo "------------------------"
cd tests/codec_tests
cargo test -- --nocapture
cd ../../

echo
echo "2. JWS Verification"
echo "------------------------"
cd crates/common/icn-crypto
cargo test --test jws_roundtrip -- --nocapture
cd ../../../

echo
echo "3. DID Key Verification"
echo "------------------------"
cd crates/common/icn-identity-core
cargo test --test did_key_test -- --nocapture
cd ../../../

echo
echo "✅ All verification tests completed successfully!"
echo
echo "These tests confirm that our core primitives are working correctly"
echo "and should be compatible with external implementations." 