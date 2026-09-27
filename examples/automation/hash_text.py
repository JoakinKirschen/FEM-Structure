# Executed by the restricted Pass 13 Python planner.
text = str(context.get("text", ""))
operation_index = api.call("artifact.sha256", {"text": text})
emit({"queued_operation": operation_index, "characters": len(text)})
