namespace Structural.Desktop.Core.Palette;

public sealed record PaletteCommand(
    string Id,
    string Title,
    string Category,
    Func<CancellationToken, Task> ExecuteAsync,
    string[]? Keywords = null);
