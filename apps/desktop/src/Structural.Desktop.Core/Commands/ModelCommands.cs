using Structural.Desktop.Core.Model;

namespace Structural.Desktop.Core.Commands;

public sealed class AddNodeCommand(StructuralModel model, StructuralNode node) : IUndoableCommand
{
    public string Description => $"Add node {node.Id}";
    public void Execute() => model.AddNode(node);
    public void Undo() => model.RemoveNode(node.Id);
}

public sealed class MoveNodeCommand(
    StructuralModel model,
    string nodeId,
    ModelPoint newPosition) : IUndoableCommand
{
    private StructuralNode? _previous;

    public string Description => $"Move node {nodeId}";

    public void Execute()
    {
        _previous ??= model.Nodes.FirstOrDefault(x => x.Id == nodeId)
            ?? throw new KeyNotFoundException($"Node '{nodeId}' was not found.");
        model.ReplaceNode(_previous with { Position = newPosition });
    }

    public void Undo()
    {
        if (_previous is not null)
            model.ReplaceNode(_previous);
    }
}

public sealed class AddMemberCommand(StructuralModel model, StructuralMember member) : IUndoableCommand
{
    public string Description => $"Add member {member.Id}";
    public void Execute() => model.AddMember(member);
    public void Undo() => model.RemoveMember(member.Id);
}

public sealed class RemoveMemberCommand(
    StructuralModel model,
    string memberId,
    WorkspaceState? workspace = null) : IUndoableCommand
{
    private StructuralMember? _removed;
    private IReadOnlyList<ResourceAssignment> _removedAssignments = [];

    public string Description => $"Delete member {memberId}";

    public void Execute()
    {
        _removed = model.RemoveMember(memberId);
        if (workspace is null)
            return;
        _removedAssignments = workspace.Assignments.Where(x => x.TargetId == memberId).ToList();
        foreach (var assignment in _removedAssignments)
            workspace.RemoveAssignment(assignment.TargetId, assignment.Kind);
    }

    public void Undo()
    {
        if (_removed is not null)
            model.AddMember(_removed);
        if (workspace is not null)
            foreach (var assignment in _removedAssignments)
                workspace.SetAssignment(assignment);
    }
}

public sealed class RemoveNodeCommand(
    StructuralModel model,
    string nodeId,
    bool cascadeMembers = true,
    WorkspaceState? workspace = null) : IUndoableCommand
{
    private StructuralNode? _removedNode;
    private IReadOnlyList<StructuralMember> _removedMembers = [];
    private IReadOnlyList<ResourceAssignment> _removedAssignments = [];

    public string Description => $"Delete node {nodeId}";

    public void Execute()
    {
        _removedNode ??= model.Nodes.FirstOrDefault(x => x.Id == nodeId)
            ?? throw new KeyNotFoundException($"Node '{nodeId}' was not found.");
        _removedMembers = model.Members.Where(x =>
            x.StartNodeId == nodeId || x.EndNodeId == nodeId).ToList();
        var removedIds = _removedMembers.Select(x => x.Id).Append(nodeId).ToHashSet(StringComparer.Ordinal);
        if (workspace is not null)
        {
            _removedAssignments = workspace.Assignments.Where(x => removedIds.Contains(x.TargetId)).ToList();
            foreach (var assignment in _removedAssignments)
                workspace.RemoveAssignment(assignment.TargetId, assignment.Kind);
        }
        model.RemoveNode(nodeId, cascadeMembers);
    }

    public void Undo()
    {
        if (_removedNode is null)
            return;
        model.AddNode(_removedNode);
        foreach (var member in _removedMembers)
            model.AddMember(member);
        if (workspace is not null)
            foreach (var assignment in _removedAssignments)
                workspace.SetAssignment(assignment);
    }
}
