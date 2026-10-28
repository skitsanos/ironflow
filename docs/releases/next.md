# Next IronFlow Release - Draft

The approved 1.20.0 changes are recorded in [1.20.0.md](1.20.0.md).
Development versions remain prereleases until release-branch finalization;
publication is verified separately.

Development continues as `1.20.1-dev.5`.

## Dependencies And Security

- Update AWS S3 to `1.156.0`, S3 Vectors to `1.46.0`, and the shared Smithy
  runtime cohort, including `aws-sigv4 1.6.4`.
- Restore SigV4a for supported S3 Multi-Region Access Point object operations
  and presigned URLs. Require S3 SDK `>=1.156.0`, signer `>=1.6.4`, and patched
  `crypto-bigint 0.7.10`; preserve regional SigV4 and fail-closed audit policy.
  See [security guidance](../SECURITY.md#s3-signing) for limits and the
  [MRAP example](../../examples/04-file-operations/s3_mrap_presign.lua).
- Update ZIP to `9.1.3`, including AES key zeroization, ZIP64 offset and
  archive metadata fixes, and Windows symlink-safe extraction improvements.

## Container Publication

- Publish new container builds to Docker Hub alongside GHCR and promote
  verified images to stable version tags after successful releases. See
  [Docker publishing](../DOCKER_PUBLISHING.md) for configuration and limits.

Earlier maintenance changes remain in [the 1.19.2 notes](1.19.2.md).
