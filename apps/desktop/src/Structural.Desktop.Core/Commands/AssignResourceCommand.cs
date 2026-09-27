using Structural.Desktop.Core.Model;

namespace Structural.Desktop.Core.Commands;

public sealed class AssignResourceCommand : IUndoableCommand
{
    private readonly WorkspaceState _workspace;
    private readonly ResourceAssignment _next;
    private ResourceAssignment? _previous;
    private bool _captured;

    public AssignResourceCommand(WorkspaceState workspace, string targetId, ResourcePayload payload)
    {
        _workspace = workspace;
        _next = new ResourceAssignment(targetId, payload.Kind, payload.ResourceId,
            payload.DisplayName, DateTimeOffset.UtcNow, payload.MagnitudeN,
            payload.DirectionX, payload.DirectionY, payload.DirectionZ);
    }

    public AssignResourceCommand(WorkspaceState workspace, ResourceAssignment assignment)
    {
        _workspace = workspace;
        _next = assignment with { AssignedAt = DateTimeOffset.UtcNow };
    }

    public string Description => $"Assign {_next.DisplayName} to {_next.TargetId}";

    public void Execute()
    {
        if (!_captured)
        {
            _previous = _workspace.GetAssignment(_next.TargetId, _next.Kind);
            _captured = true;
        }
        _workspace.SetAssignment(_next);
    }

    public void Undo()
    {
        if (_previous is null)
            _workspace.RemoveAssignment(_next.TargetId, _next.Kind);
        else
            _workspace.SetAssignment(_previous);
    }
}
