const { CID } = require('multiformats/cid')
const golden = "zdpuAwrkZe6cjfJ1c7oD5hWkwZXETu9G9LQVMjJjW1JQbRJZs"
try {
  const cid = CID.parse(golden)
  console.log("Valid CID!")
  console.log(`Codec: ${cid.code} (${cid.code === 0x71 ? 'dag-cbor ✓' : 'not dag-cbor ✗'})`)
  console.log(`Hash: ${cid.multihash.digest.toString('hex')}`)
} catch (e) {
  console.error("Invalid CID:", e.message)
}
