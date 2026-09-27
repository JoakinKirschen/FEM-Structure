using System.Text.Json;

namespace Structural.Desktop.Core.Model;

public static class DocumentLoader
{
    public static async Task<ModelNode> LoadAsync(string path, CancellationToken cancellationToken = default)
    {
        await using var stream = File.OpenRead(path);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
        return BuildNode(document.RootElement, "root", Path.GetFileName(path), "document", 0);
    }

    private static ModelNode BuildNode(JsonElement value, string id, string name, string kind, int depth)
    {
        if (depth >= 5)
            return new ModelNode(id, name, kind);

        var children = new List<ModelNode>();
        switch (value.ValueKind)
        {
            case JsonValueKind.Object:
                foreach (var property in value.EnumerateObject()
                             .OrderBy(x => x.Name, StringComparer.Ordinal))
                {
                    if (property.Value.ValueKind is JsonValueKind.Object or JsonValueKind.Array)
                    {
                        children.Add(BuildNode(property.Value, $"{id}/{Escape(property.Name)}",
                            Friendly(property.Name), property.Value.ValueKind.ToString().ToLowerInvariant(), depth + 1));
                    }
                }

                var entityId = TryString(value, "id") ?? TryString(value, "uuid") ?? id;
                var entityName = TryString(value, "name") ?? name;
                return new ModelNode(entityId, entityName, kind, children);

            case JsonValueKind.Array:
                var index = 0;
                foreach (var item in value.EnumerateArray())
                {
                    var itemId = item.ValueKind == JsonValueKind.Object
                        ? TryString(item, "id") ?? TryString(item, "uuid") ?? $"{id}/{index}"
                        : $"{id}/{index}";
                    var itemName = item.ValueKind == JsonValueKind.Object
                        ? TryString(item, "name") ?? $"{Singular(name)} {index + 1}"
                        : $"{Singular(name)} {index + 1}";
                    children.Add(BuildNode(item, itemId, itemName, Singular(kind), depth + 1));
                    index++;
                }
                return new ModelNode(id, $"{name} ({children.Count})", kind, children);

            default:
                return new ModelNode(id, name, kind);
        }
    }

    private static string? TryString(JsonElement value, string propertyName) =>
        value.TryGetProperty(propertyName, out var property) &&
        property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private static string Friendly(string value) =>
        string.Join(" ", value.Split(['_', '-'], StringSplitOptions.RemoveEmptyEntries)
            .Select(x => char.ToUpperInvariant(x[0]) + x[1..]));

    private static string Singular(string value) =>
        value.EndsWith("s", StringComparison.OrdinalIgnoreCase) ? value[..^1] : value;

    private static string Escape(string value) => value.Replace("~", "~0").Replace("/", "~1");
}
