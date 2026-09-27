using Structural.Desktop.Core.Model;

namespace Structural.Desktop.Core.Commands;

public sealed class RemoveResourceCommand : IUndoableCommand
{
    private readonly WorkspaceState _workspace;
    private readonly string _targetId;
    private readonly ResourceKind _kind;
    private ResourceAssignment? _removed;

    public RemoveResourceCommand(WorkspaceState workspace, string targetId, ResourceKind kind)
    {
        _workspace = workspace;
        _targetId = targetId;
        _kind = kind;
    }

    public string Description => $"Remove {_kind} from {_targetId}";

    public void Execute()
    {
        _removed ??= _workspace.GetAssignment(_targetId, _kind);
        _workspace.RemoveAssignment(_targetId, _kind);
    }

    public void Undo()
    {
        if (_removed is not null)
            _workspace.SetAssignment(_removed);
    }
}
