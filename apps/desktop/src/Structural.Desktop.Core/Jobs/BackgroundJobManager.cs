using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Text;

namespace Structural.Desktop.Core.Jobs;

public sealed class BackgroundJobManager
{
    private readonly List<BackgroundJob> _jobs = [];
    private readonly object _gate = new();

    public event EventHandler? Changed;

    public IReadOnlyList<BackgroundJob> Jobs
    {
        get
        {
            lock (_gate)
                return new ReadOnlyCollection<BackgroundJob>(_jobs.ToList());
        }
    }

    public BackgroundJob StartProcess(string name, string executable, IEnumerable<string> arguments,
        string workingDirectory, int maximumOutputCharacters = 200_000)
    {
        var job = new BackgroundJob(name);
        lock (_gate)
            _jobs.Insert(0, job);
        Changed?.Invoke(this, EventArgs.Empty);
        _ = RunProcessAsync(job, executable, arguments, workingDirectory, maximumOutputCharacters);
        return job;
    }

    public bool Cancel(Guid id)
    {
        BackgroundJob? job;
        lock (_gate)
            job = _jobs.SingleOrDefault(x => x.Id == id);
        if (job is null || job.Status is not (BackgroundJobStatus.Queued or BackgroundJobStatus.Running))
            return false;
        job.Cancellation.Cancel();
        return true;
    }

    private async Task RunProcessAsync(BackgroundJob job, string executable,
        IEnumerable<string> arguments, string workingDirectory, int maximumOutputCharacters)
    {
        try
        {
            job.Status = BackgroundJobStatus.Running;
            job.StartedAt = DateTimeOffset.UtcNow;
            Changed?.Invoke(this, EventArgs.Empty);

            var info = new ProcessStartInfo(executable)
            {
                WorkingDirectory = workingDirectory,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true
            };
            foreach (var argument in arguments)
                info.ArgumentList.Add(argument);

            using var process = new Process { StartInfo = info };
            var output = new StringBuilder();
            process.OutputDataReceived += (_, e) => Append(output, e.Data, maximumOutputCharacters);
            process.ErrorDataReceived += (_, e) => Append(output, e.Data, maximumOutputCharacters);

            if (!process.Start())
                throw new InvalidOperationException($"Could not start '{executable}'.");

            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            using var registration = job.Cancellation.Token.Register(() =>
            {
                try
                {
                    if (!process.HasExited)
                        process.Kill(entireProcessTree: true);
                }
                catch (InvalidOperationException) { }
            });

            await process.WaitForExitAsync(job.Cancellation.Token);
            job.ExitCode = process.ExitCode;
            job.Status = process.ExitCode == 0 ? BackgroundJobStatus.Succeeded : BackgroundJobStatus.Failed;
            job.Output = output.ToString();
        }
        catch (OperationCanceledException)
        {
            job.Status = BackgroundJobStatus.Cancelled;
        }
        catch (Exception exception)
        {
            job.Status = BackgroundJobStatus.Failed;
            job.Output = exception.ToString();
        }
        finally
        {
            job.CompletedAt = DateTimeOffset.UtcNow;
            Changed?.Invoke(this, EventArgs.Empty);
        }
    }

    private static void Append(StringBuilder output, string? line, int limit)
    {
        if (line is null || output.Length >= limit)
            return;
        var remaining = limit - output.Length;
        output.AppendLine(line.Length <= remaining ? line : line[..remaining]);
    }
}
