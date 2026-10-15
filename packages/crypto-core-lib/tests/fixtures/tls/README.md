These are public, synthetic TLS test fixtures. `server-key.der` is deliberately
committed test data, unrelated to any application/device/production identity.
The CA signing key was generated in a temporary directory and is not committed.
The server certificate covers `localhost` and `127.0.0.1`; it is valid for ten
years from October 15, 2026. Tests trust this CA only in their own client instance.
They never install roots into the OS or disable certificate/hostname validation.

To regenerate, create a throwaway RSA CA using `openssl req -x509`, then a separate
RSA server CSR, and sign the CSR using `openssl x509 -req -CA ... -CAkey ...`.
Use critical `basicConstraints=CA:FALSE`, critical
`keyUsage=digitalSignature,keyEncipherment`, `extendedKeyUsage=serverAuth`, and
`subjectAltName=DNS:localhost,IP:127.0.0.1`. Export the CA as PEM and DER, the server
certificate as DER, and the server private key as unencrypted PKCS#8 DER.
Keep the signing key temporary; these files are solely a loopback test identity.
