# Test signing keys

`test_rsa.pem` and `test_key.p8` are public, test-only signing fixtures used by
the deterministic FCM/APNs unit tests with a fake HTTP transport. They are not
production credentials and must never be configured for real service accounts,
push providers, or deployed environments.
