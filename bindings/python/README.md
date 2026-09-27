# Generated Python binding

This dependency-free Python 3.10+ client targets Structural Automation API `1.1`.

```python
from structural_api import StructuralAutomationClient

client = StructuralAutomationClient("./target/debug/structural-automation")
print(client.describe())
```

Add `bindings/python` to `PYTHONPATH`, or package this directory using your
organization's normal Python build process. Regenerate with:

```bash
python3 bindings/python/generate.py
```

The client invokes the local JSON-lines transport. It does not grant capabilities
implicitly: each generated convenience method sends only its declared capability.
