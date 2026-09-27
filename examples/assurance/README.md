# Assurance example

Build the static, hash-verified portal with:

```bash
cargo run -p structural-cli -- assurance-build \
  examples/assurance/catalog.json \
  examples/assurance \
  ./assurance-portal
```

Open `assurance-portal/index.html` locally. The included “lab report” is explicitly
an unverified placeholder and must not be represented as independent assurance.
