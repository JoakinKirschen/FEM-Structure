namespace Structural.Desktop.Core.Jobs;

public enum BackgroundJobStatus
{
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled
}

public sealed class BackgroundJob
{
    internal BackgroundJob(string name)
    {
        Id = Guid.NewGuid();
        Name = name;
        Status = BackgroundJobStatus.Queued;
        CreatedAt = DateTimeOffset.UtcNow;
    }

    public Guid Id { get; }
    public string Name { get; }
    public BackgroundJobStatus Status { get; internal set; }
    public DateTimeOffset CreatedAt { get; }
    public DateTimeOffset? StartedAt { get; internal set; }
    public DateTimeOffset? CompletedAt { get; internal set; }
    public int? ExitCode { get; internal set; }
    public string Output { get; internal set; } = string.Empty;
    internal CancellationTokenSource Cancellation { get; } = new();
}
