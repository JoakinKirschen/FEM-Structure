namespace Structural.Desktop.Core.Palette;

public sealed class CommandPalette
{
    private readonly List<PaletteCommand> _commands = [];

    public void Register(PaletteCommand command)
    {
        if (_commands.Any(x => StringComparer.Ordinal.Equals(x.Id, command.Id)))
            throw new InvalidOperationException($"Command '{command.Id}' is already registered.");
        _commands.Add(command);
    }

    public IReadOnlyList<PaletteCommand> Search(string? query, int maximum = 20)
    {
        var terms = (query ?? string.Empty).Split(' ', StringSplitOptions.RemoveEmptyEntries |
            StringSplitOptions.TrimEntries);

        return _commands
            .Select(command => (Command: command, Score: Score(command, terms)))
            .Where(x => x.Score >= 0)
            .OrderByDescending(x => x.Score)
            .ThenBy(x => x.Command.Title, StringComparer.OrdinalIgnoreCase)
            .Take(maximum)
            .Select(x => x.Command)
            .ToList();
    }

    private static int Score(PaletteCommand command, IReadOnlyList<string> terms)
    {
        if (terms.Count == 0)
            return 0;

        var title = command.Title;
        var haystack = $"{command.Title} {command.Category} {string.Join(' ', command.Keywords ?? [])}";
        var score = 0;
        foreach (var term in terms)
        {
            if (!haystack.Contains(term, StringComparison.OrdinalIgnoreCase))
                return -1;
            score += title.StartsWith(term, StringComparison.OrdinalIgnoreCase) ? 10 :
                title.Contains(term, StringComparison.OrdinalIgnoreCase) ? 5 : 1;
        }
        return score;
    }
}
