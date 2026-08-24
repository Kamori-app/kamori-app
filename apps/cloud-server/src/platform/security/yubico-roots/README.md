# Pinned Yubico WebAuthn attestation roots

These public trust anchors are copied verbatim from Yubico's production PKI
directory and are compiled into `cloud-server`. They are not secrets and must
not be replaced by a runtime download.

| File | Official source | SHA-256 certificate fingerprint |
| --- | --- | --- |
| `yubico-attestation-root-1.pem` | <https://developers.yubico.com/PKI/yubico-ca-1.pem> | `62760C6A6EF91679F454C8902B80FD009825B3F25DA90F1FBACE2EC6586CD5A8` |
| `yubico-u2f-root-ca.pem` | <https://developers.yubico.com/PKI/yubico-fido-ca-1.pem> | `0FA1386F80EB8713263AE5C1D84DEB455BDF08AEA50AB05503CEFEE82B092D42` |
| `yubico-fido-root-ca.pem` | <https://developers.yubico.com/PKI/yubico-fido-ca-2.pem> | `35F1A54B353BFB711E6D42ADBEB76C0E9DEAD095018E6A94783BA2192FD6FAAD` |

Yubico's root overview is
<https://developers.yubico.com/U2F/Attestation_and_Metadata/>. When Yubico adds
a production WebAuthn root, download it from that page, inspect its subject,
issuer, validity and SHA-256 fingerprint with OpenSSL, add it through code
review, and extend the parser test. Do not automatically remove an existing
root while active operator credentials may still rely on it.
