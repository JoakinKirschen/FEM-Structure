namespace Structural.Desktop.Core.Commands;

public sealed class CommandHistory
{
    private readonly Stack<IUndoableCommand> _undo = [];
    private readonly Stack<IUndoableCommand> _redo = [];

    public event EventHandler? Changed;
    public bool CanUndo => _undo.Count > 0;
    public bool CanRedo => _redo.Count > 0;
    public string? NextUndoDescription => _undo.TryPeek(out var value) ? value.Description : null;
    public string? NextRedoDescription => _redo.TryPeek(out var value) ? value.Description : null;

    public void Execute(IUndoableCommand command)
    {
        command.Execute();
        _undo.Push(command);
        _redo.Clear();
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void Undo()
    {
        if (!_undo.TryPop(out var command))
            return;
        command.Undo();
        _redo.Push(command);
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void Redo()
    {
        if (!_redo.TryPop(out var command))
            return;
        command.Execute();
        _undo.Push(command);
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void Clear()
    {
        _undo.Clear();
        _redo.Clear();
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
