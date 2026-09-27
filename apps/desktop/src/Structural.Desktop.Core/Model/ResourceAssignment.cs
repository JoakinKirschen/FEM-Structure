namespace Structural.Desktop.Core.Model;

public enum ResourceKind
{
    Load,
    Support,
    Material
}

public sealed record ResourcePayload(
    ResourceKind Kind,
    string ResourceId,
    string DisplayName,
    double MagnitudeN = 0,
    double DirectionX = 0,
    double DirectionY = 0,
    double DirectionZ = -1);

public sealed record ResourceAssignment(
    string TargetId,
    ResourceKind Kind,
    string ResourceId,
    string DisplayName,
    DateTimeOffset AssignedAt,
    double MagnitudeN = 0,
    double DirectionX = 0,
    double DirectionY = 0,
    double DirectionZ = -1);
