using System.Collections.ObjectModel;
using System.Text.Json;

namespace Structural.Desktop.Core.Model;

public sealed class WorkspaceState
{
    private readonly Dictionary<(string TargetId, ResourceKind Kind), ResourceAssignment> _assignments = [];

    public ModelNode? Root { get; private set; }
    public StructuralModel? Model { get; private set; }
    public string? SourcePath { get; private set; }
    public bool IsDirty { get; private set; }
    public IReadOnlyCollection<ResourceAssignment> Assignments => new ReadOnlyCollection<ResourceAssignment>(
        _assignments.Values.OrderBy(x => x.TargetId, StringComparer.Ordinal)
            .ThenBy(x => x.Kind).ToList());

    public void SetDocument(string sourcePath, ModelNode root, StructuralModel? model = null)
    {
        SourcePath = sourcePath;
        Root = root;
        Model = model;
        _assignments.Clear();
        IsDirty = false;
    }

    public void UpdateModelTree(ModelNode root)
    {
        Root = root;
        IsDirty = true;
    }

    public void MarkDirty() => IsDirty = true;

    public void MarkSaved(string path)
    {
        SourcePath = path;
        IsDirty = false;
    }

    public ResourceAssignment? GetAssignment(string targetId, ResourceKind kind) =>
        _assignments.GetValueOrDefault((targetId, kind));

    public void SetAssignment(ResourceAssignment assignment)
    {
        _assignments[(assignment.TargetId, assignment.Kind)] = assignment;
        IsDirty = true;
    }

    public bool RemoveAssignment(string targetId, ResourceKind kind)
    {
        var removed = _assignments.Remove((targetId, kind));
        IsDirty |= removed;
        return removed;
    }

    public async Task SaveAssignmentsAsync(string path, CancellationToken cancellationToken = default)
    {
        var envelope = new
        {
            schema = "structural-desktop-assignments/0.2",
            source = SourcePath,
            assignments = Assignments
        };
        await using var stream = File.Create(path);
        await JsonSerializer.SerializeAsync(stream, envelope,
            new JsonSerializerOptions { WriteIndented = true }, cancellationToken);
    }
}
