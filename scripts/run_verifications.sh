#!/bin/bash
set -e

echo "=========================================="
echo "ICN Core Components Verification"
echo "=========================================="
echo

echo "1. CID PARITY VERIFICATION"
echo "-------------------------------------------"
cargo test --test codec_verification -- --nocapture
echo

echo "2. JWS VERIFICATION"
echo "-------------------------------------------"
cargo test -p icn-crypto --test jws_roundtrip -- --nocapture
echo

echo "3. DID KEY VERIFICATION"
echo "-------------------------------------------"
cargo test -p icn-identity-core --test did_key_test -- --nocapture
echo

echo "=========================================="
echo "✅ ALL VERIFICATIONS PASSED"
echo "=========================================="
echo
echo "Thread              Status       Compatibility"
echo "------------------- ------------ -----------------------"
echo "CID parity          ✓ PASS       Compatible with IPLD spec"
echo "Detached JWS        ✓ PASS       RFC 7515 compliant"
echo "DID key utils       ✓ PASS       W3C DID spec compliant"
echo
echo "These components are now ready for federation integration." 