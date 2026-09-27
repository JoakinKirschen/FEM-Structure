using System.Text.Json;

namespace Structural.Desktop.Core.Model;

public readonly record struct ModelPoint(double X, double Y, double Z)
{
    public static ModelPoint operator +(ModelPoint a, ModelPoint b) =>
        new(a.X + b.X, a.Y + b.Y, a.Z + b.Z);
    public static ModelPoint operator *(ModelPoint value, double scale) =>
        new(value.X * scale, value.Y * scale, value.Z * scale);
}

public sealed record StructuralNode(string Id, string Name, ModelPoint Position);

public sealed record StructuralMember(
    string Id,
    string Name,
    string StartNodeId,
    string EndNodeId,
    double AreaM2 = 0.01,
    double YoungsModulusPa = 210_000_000_000);

public sealed record ModelDiagnostic(string Severity, string Code, string Message, string? TargetId = null);

public sealed class StructuralModel
{
    private readonly List<StructuralNode> _nodes;
    private readonly List<StructuralMember> _members;
    private readonly List<ModelDiagnostic> _parseDiagnostics;

    public StructuralModel(
        string name,
        IEnumerable<StructuralNode> nodes,
        IEnumerable<StructuralMember> members,
        IEnumerable<ModelDiagnostic>? diagnostics = null)
    {
        Name = string.IsNullOrWhiteSpace(name) ? "Structural model" : name.Trim();
        _nodes = nodes.ToList();
        _members = members.ToList();
        _parseDiagnostics = diagnostics?.ToList() ?? [];
        Diagnostics = Validate().ToList();
    }

    public string Name { get; private set; }
    public IReadOnlyList<StructuralNode> Nodes => _nodes;
    public IReadOnlyList<StructuralMember> Members => _members;
    public IReadOnlyList<ModelDiagnostic> Diagnostics { get; private set; }
    public bool IsValid => Diagnostics.All(x =>
        !x.Severity.Equals("Error", StringComparison.OrdinalIgnoreCase));

    public void Rename(string name)
    {
        if (string.IsNullOrWhiteSpace(name))
            throw new ArgumentException("Model name cannot be empty.", nameof(name));
        Name = name.Trim();
    }

    public void AddNode(StructuralNode node)
    {
        ValidateId(node.Id, "Node");
        if (!IsFinite(node.Position))
            throw new ArgumentException("Node coordinates must be finite.", nameof(node));
        if (_nodes.Any(x => x.Id.Equals(node.Id, StringComparison.Ordinal)))
            throw new InvalidOperationException($"Node id '{node.Id}' already exists.");
        _nodes.Add(node);
        RefreshDiagnostics();
    }

    public void ReplaceNode(StructuralNode node)
    {
        ValidateId(node.Id, "Node");
        if (!IsFinite(node.Position))
            throw new ArgumentException("Node coordinates must be finite.", nameof(node));
        var index = _nodes.FindIndex(x => x.Id.Equals(node.Id, StringComparison.Ordinal));
        if (index < 0)
            throw new KeyNotFoundException($"Node '{node.Id}' was not found.");
        _nodes[index] = node;
        RefreshDiagnostics();
    }

    public StructuralNode RemoveNode(string id, bool cascadeMembers = false)
    {
        var node = _nodes.FirstOrDefault(x => x.Id.Equals(id, StringComparison.Ordinal))
            ?? throw new KeyNotFoundException($"Node '{id}' was not found.");
        var attached = _members.Where(x =>
            x.StartNodeId.Equals(id, StringComparison.Ordinal) ||
            x.EndNodeId.Equals(id, StringComparison.Ordinal)).ToList();
        if (attached.Count > 0 && !cascadeMembers)
            throw new InvalidOperationException(
                $"Node '{id}' is used by {attached.Count} member(s). Delete them first or use cascade deletion.");
        foreach (var member in attached)
            _members.Remove(member);
        _nodes.Remove(node);
        RefreshDiagnostics();
        return node;
    }

    public void AddMember(StructuralMember member)
    {
        ValidateId(member.Id, "Member");
        if (_members.Any(x => x.Id.Equals(member.Id, StringComparison.Ordinal)))
            throw new InvalidOperationException($"Member id '{member.Id}' already exists.");
        if (!_nodes.Any(x => x.Id.Equals(member.StartNodeId, StringComparison.Ordinal)) ||
            !_nodes.Any(x => x.Id.Equals(member.EndNodeId, StringComparison.Ordinal)))
            throw new InvalidOperationException("Both member end nodes must exist.");
        if (member.StartNodeId.Equals(member.EndNodeId, StringComparison.Ordinal))
            throw new InvalidOperationException("A member requires two different nodes.");
        if (!(member.AreaM2 > 0) || !(member.YoungsModulusPa > 0) ||
            !double.IsFinite(member.AreaM2) || !double.IsFinite(member.YoungsModulusPa))
            throw new InvalidOperationException("Member area and Young's modulus must be finite and positive.");
        _members.Add(member);
        RefreshDiagnostics();
    }

    public void ReplaceMember(StructuralMember member)
    {
        var index = _members.FindIndex(x => x.Id.Equals(member.Id, StringComparison.Ordinal));
        if (index < 0)
            throw new KeyNotFoundException($"Member '{member.Id}' was not found.");
        var previous = _members[index];
        _members.RemoveAt(index);
        try
        {
            AddMember(member);
            var added = _members[^1];
            _members.RemoveAt(_members.Count - 1);
            _members.Insert(index, added);
            RefreshDiagnostics();
        }
        catch
        {
            _members.Insert(index, previous);
            RefreshDiagnostics();
            throw;
        }
    }

    public StructuralMember RemoveMember(string id)
    {
        var member = _members.FirstOrDefault(x => x.Id.Equals(id, StringComparison.Ordinal))
            ?? throw new KeyNotFoundException($"Member '{id}' was not found.");
        _members.Remove(member);
        RefreshDiagnostics();
        return member;
    }

    public string NextId(string prefix)
    {
        var used = _nodes.Select(x => x.Id).Concat(_members.Select(x => x.Id))
            .ToHashSet(StringComparer.Ordinal);
        for (var index = 1; ; index++)
        {
            var candidate = $"{prefix}-{index}";
            if (!used.Contains(candidate))
                return candidate;
        }
    }

    public async Task SaveAsync(string path, CancellationToken cancellationToken = default)
    {
        var envelope = new
        {
            schema_version = "0.4-desktop",
            name = Name,
            nodes = _nodes.Select(x => new
            {
                id = x.Id,
                name = x.Name,
                position = new { x = x.Position.X, y = x.Position.Y, z = x.Position.Z }
            }),
            members = _members.Select(x => new
            {
                id = x.Id,
                name = x.Name,
                start_node_id = x.StartNodeId,
                end_node_id = x.EndNodeId,
                area_m2 = x.AreaM2,
                youngs_modulus_pa = x.YoungsModulusPa
            })
        };
        await using var stream = File.Create(path);
        await JsonSerializer.SerializeAsync(stream, envelope,
            new JsonSerializerOptions { WriteIndented = true }, cancellationToken);
    }

    public static StructuralModel Starter()
    {
        var nodes = new[]
        {
            new StructuralNode("node-a", "Left support", new ModelPoint(0, 0, 0)),
            new StructuralNode("node-b", "Right support", new ModelPoint(6, 0, 0)),
            new StructuralNode("node-c", "Apex", new ModelPoint(3, 0, 2.5))
        };
        var members = new[]
        {
            new StructuralMember("member-1", "Left chord", "node-a", "node-c"),
            new StructuralMember("member-2", "Right chord", "node-c", "node-b"),
            new StructuralMember("member-3", "Bottom chord", "node-a", "node-b")
        };
        return new StructuralModel("Starter truss", nodes, members);
    }

    public static async Task<StructuralModel> LoadAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        await using var stream = File.OpenRead(path);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
        return Parse(document.RootElement);
    }

    public static StructuralModel Parse(JsonElement root)
    {
        var diagnostics = new List<ModelDiagnostic>();
        var nodes = new List<StructuralNode>();
        var members = new List<StructuralMember>();
        var name = ReadString(root, "name") ?? "Structural model";

        if (!TryGetProperty(root, "nodes", out var nodeArray) || nodeArray.ValueKind != JsonValueKind.Array)
        {
            diagnostics.Add(new("Error", "model.nodes.missing", "The document has no nodes array."));
        }
        else
        {
            foreach (var value in nodeArray.EnumerateArray())
            {
                var id = ReadString(value, "id") ?? ReadString(value, "uuid");
                if (string.IsNullOrWhiteSpace(id))
                {
                    diagnostics.Add(new("Error", "node.id.missing", "A node has no id."));
                    continue;
                }

                if (!TryReadPosition(value, out var position))
                {
                    diagnostics.Add(new("Error", "node.position.invalid",
                        "Node position must use position {x,y,z} or xyz_m [x,y,z].", id));
                    continue;
                }

                nodes.Add(new StructuralNode(id, ReadString(value, "name") ?? id, position));
            }
        }

        if (TryGetProperty(root, "members", out var memberArray) && memberArray.ValueKind == JsonValueKind.Array)
        {
            foreach (var value in memberArray.EnumerateArray())
            {
                var id = ReadString(value, "id") ?? ReadString(value, "uuid");
                var start = ReadString(value, "start_node_id") ?? ReadString(value, "startNodeId")
                    ?? ReadString(value, "start_node");
                var end = ReadString(value, "end_node_id") ?? ReadString(value, "endNodeId")
                    ?? ReadString(value, "end_node");
                if (string.IsNullOrWhiteSpace(id) || string.IsNullOrWhiteSpace(start) ||
                    string.IsNullOrWhiteSpace(end))
                {
                    diagnostics.Add(new("Error", "member.reference.missing",
                        "A member requires id, start_node_id and end_node_id.", id));
                    continue;
                }

                var area = ReadNumber(value, "area_m2") ?? 0.01;
                var modulus = ReadNumber(value, "youngs_modulus_pa") ?? 210_000_000_000;
                members.Add(new StructuralMember(id, ReadString(value, "name") ?? id,
                    start, end, area, modulus));
            }
        }

        return new StructuralModel(name, nodes, members, diagnostics);
    }

    private IEnumerable<ModelDiagnostic> Validate()
    {
        foreach (var diagnostic in _parseDiagnostics)
            yield return diagnostic;

        var nodeIds = _nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
        foreach (var member in _members)
        {
            if (!nodeIds.Contains(member.StartNodeId) || !nodeIds.Contains(member.EndNodeId))
                yield return new("Error", "member.node.dangling",
                    "Member references a node that does not exist.", member.Id);
            if (member.StartNodeId == member.EndNodeId)
                yield return new("Error", "member.zero.reference",
                    "Member start and end node are identical.", member.Id);
            if (!(member.AreaM2 > 0) || !(member.YoungsModulusPa > 0))
                yield return new("Error", "member.property.invalid",
                    "Member area and Young's modulus must be positive.", member.Id);
        }

        foreach (var duplicate in _nodes.GroupBy(x => x.Id, StringComparer.Ordinal).Where(x => x.Count() > 1))
            yield return new("Error", "node.id.duplicate", $"Duplicate node id '{duplicate.Key}'.", duplicate.Key);
        foreach (var duplicate in _members.GroupBy(x => x.Id, StringComparer.Ordinal).Where(x => x.Count() > 1))
            yield return new("Error", "member.id.duplicate", $"Duplicate member id '{duplicate.Key}'.", duplicate.Key);

        if (_nodes.Count == 0)
            yield return new("Error", "model.empty", "No usable nodes were found.");
        if (_members.Count == 0)
            yield return new("Warning", "model.members.empty", "No line members were found.");
    }

    private void RefreshDiagnostics() => Diagnostics = Validate().ToList();

    private static void ValidateId(string id, string kind)
    {
        if (string.IsNullOrWhiteSpace(id))
            throw new ArgumentException($"{kind} id cannot be empty.");
    }

    private static bool TryReadPosition(JsonElement node, out ModelPoint point)
    {
        point = default;
        if (TryGetProperty(node, "position", out var position) &&
            position.ValueKind == JsonValueKind.Object &&
            ReadNumber(position, "x") is double x &&
            ReadNumber(position, "y") is double y &&
            ReadNumber(position, "z") is double z)
        {
            point = new(x, y, z);
            return IsFinite(point);
        }

        if (TryGetProperty(node, "xyz_m", out var xyz) && xyz.ValueKind == JsonValueKind.Array)
        {
            var values = xyz.EnumerateArray().Take(3).Select(ReadNumber).ToArray();
            if (values.Length == 3 && values.All(x => x.HasValue))
            {
                point = new(values[0]!.Value, values[1]!.Value, values[2]!.Value);
                return IsFinite(point);
            }
        }
        return false;
    }

    private static bool IsFinite(ModelPoint point) =>
        double.IsFinite(point.X) && double.IsFinite(point.Y) && double.IsFinite(point.Z);

    private static string? ReadString(JsonElement value, string name) =>
        TryGetProperty(value, name, out var property) && property.ValueKind == JsonValueKind.String
            ? property.GetString()
            : null;

    private static double? ReadNumber(JsonElement value, string name) =>
        TryGetProperty(value, name, out var property) ? ReadNumber(property) : null;

    private static double? ReadNumber(JsonElement value) =>
        value.ValueKind == JsonValueKind.Number && value.TryGetDouble(out var number) ? number : null;

    private static bool TryGetProperty(JsonElement value, string name, out JsonElement property)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            foreach (var candidate in value.EnumerateObject())
            {
                if (candidate.Name.Equals(name, StringComparison.OrdinalIgnoreCase))
                {
                    property = candidate.Value;
                    return true;
                }
            }
        }
        property = default;
        return false;
    }
}
