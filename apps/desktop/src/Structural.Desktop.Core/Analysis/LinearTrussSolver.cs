using Structural.Desktop.Core.Model;

namespace Structural.Desktop.Core.Analysis;

public sealed record NodeResult(string NodeId, double DisplacementXM, double DisplacementZM,
    double ReactionXN, double ReactionZN);

public sealed record MemberResult(string MemberId, double AxialForceN);

public sealed class LinearTrussResult
{
    public required IReadOnlyList<NodeResult> Nodes { get; init; }
    public required IReadOnlyList<MemberResult> Members { get; init; }
    public required double MaximumDisplacementM { get; init; }
    public required IReadOnlyList<string> Warnings { get; init; }
}

public static class LinearTrussSolver
{
    public static LinearTrussResult Solve(
        StructuralModel model,
        IEnumerable<ResourceAssignment> assignments)
    {
        if (!model.IsValid)
            throw new InvalidOperationException("The model contains validation errors.");
        if (model.Nodes.Count == 0 || model.Members.Count == 0)
            throw new InvalidOperationException("Analysis requires nodes and members.");

        var assignmentList = assignments.ToList();
        var materialByTarget = assignmentList
            .Where(x => x.Kind == ResourceKind.Material)
            .ToDictionary(x => x.TargetId, x => x.ResourceId, StringComparer.Ordinal);
        var nodes = model.Nodes.ToList();
        var nodeIndex = nodes.Select((node, index) => (node.Id, index))
            .ToDictionary(x => x.Id, x => x.index, StringComparer.Ordinal);
        var dofCount = nodes.Count * 2;
        var stiffness = new double[dofCount, dofCount];
        var force = new double[dofCount];
        var warnings = new List<string>();

        foreach (var member in model.Members)
        {
            if (!nodeIndex.TryGetValue(member.StartNodeId, out var startIndex) ||
                !nodeIndex.TryGetValue(member.EndNodeId, out var endIndex))
                continue;
            var start = nodes[startIndex].Position;
            var end = nodes[endIndex].Position;
            var dx = end.X - start.X;
            var dz = end.Z - start.Z;
            var length = Math.Sqrt(dx * dx + dz * dz);
            if (length <= 1e-9)
                throw new InvalidOperationException($"Member '{member.Id}' has zero projected X-Z length.");
            if (Math.Abs(end.Y - start.Y) > Math.Max(length * 1e-6, 1e-9))
                warnings.Add($"Member '{member.Id}' was projected onto the X-Z analysis plane.");

            var c = dx / length;
            var s = dz / length;
            var factor = EffectiveYoungsModulus(member, materialByTarget) *
                         member.AreaM2 / length;
            var local = new[,]
            {
                { c*c, c*s, -c*c, -c*s },
                { c*s, s*s, -c*s, -s*s },
                { -c*c, -c*s, c*c, c*s },
                { -c*s, -s*s, c*s, s*s }
            };
            var map = new[] { startIndex * 2, startIndex * 2 + 1, endIndex * 2, endIndex * 2 + 1 };
            for (var row = 0; row < 4; row++)
            for (var column = 0; column < 4; column++)
                stiffness[map[row], map[column]] += factor * local[row, column];
        }

        if (materialByTarget.Count > 0)
            warnings.Add("Assigned steel/concrete overrides Young's modulus with 210/30 GPa; member area still comes from area_m2.");

        foreach (var load in assignmentList.Where(x => x.Kind == ResourceKind.Load))
        {
            if (load.ResourceId == "gravity")
                warnings.Add("Gravity is currently interpreted as the entered total equivalent member force, split between end nodes.");
            var magnitude = load.MagnitudeN > 0 ? load.MagnitudeN : 10_000;
            var directionLength = Math.Sqrt(load.DirectionX * load.DirectionX +
                                            load.DirectionZ * load.DirectionZ);
            var directionX = directionLength > 1e-12 ? load.DirectionX / directionLength : 0;
            var directionZ = directionLength > 1e-12 ? load.DirectionZ / directionLength : -1;

            if (nodeIndex.TryGetValue(load.TargetId, out var index))
            {
                force[index * 2] += magnitude * directionX;
                force[index * 2 + 1] += magnitude * directionZ;
                continue;
            }

            var member = model.Members.FirstOrDefault(x => x.Id == load.TargetId);
            if (member is not null && nodeIndex.TryGetValue(member.StartNodeId, out var a) &&
                nodeIndex.TryGetValue(member.EndNodeId, out var b))
            {
                force[a * 2] += magnitude * directionX / 2;
                force[a * 2 + 1] += magnitude * directionZ / 2;
                force[b * 2] += magnitude * directionX / 2;
                force[b * 2 + 1] += magnitude * directionZ / 2;
            }
        }

        var constrained = new HashSet<int>();
        foreach (var support in assignmentList.Where(x => x.Kind == ResourceKind.Support))
        {
            if (!nodeIndex.TryGetValue(support.TargetId, out var index))
                continue;
            switch (support.ResourceId)
            {
                case "roller":
                    constrained.Add(index * 2 + 1);
                    break;
                case "fixed":
                case "pinned":
                    constrained.Add(index * 2);
                    constrained.Add(index * 2 + 1);
                    break;
            }
        }

        if (constrained.Count == 0)
            throw new InvalidOperationException("Assign supports before running analysis.");
        if (!force.Any(x => Math.Abs(x) > 1e-12))
            throw new InvalidOperationException("Assign at least one load before running analysis.");

        var originalStiffness = (double[,])stiffness.Clone();
        var originalForce = (double[])force.Clone();
        foreach (var dof in constrained)
        {
            for (var i = 0; i < dofCount; i++)
            {
                stiffness[dof, i] = 0;
                stiffness[i, dof] = 0;
            }
            stiffness[dof, dof] = 1;
            force[dof] = 0;
        }

        var displacement = SolveLinearSystem(stiffness, force);
        var reactions = Multiply(originalStiffness, displacement)
            .Select((value, index) => value - originalForce[index]).ToArray();

        var nodeResults = nodes.Select((node, index) => new NodeResult(
            node.Id,
            displacement[index * 2],
            displacement[index * 2 + 1],
            constrained.Contains(index * 2) ? reactions[index * 2] : 0,
            constrained.Contains(index * 2 + 1) ? reactions[index * 2 + 1] : 0)).ToList();

        var memberResults = new List<MemberResult>();
        foreach (var member in model.Members)
        {
            var i = nodeIndex[member.StartNodeId];
            var j = nodeIndex[member.EndNodeId];
            var start = nodes[i].Position;
            var end = nodes[j].Position;
            var dx = end.X - start.X;
            var dz = end.Z - start.Z;
            var length = Math.Sqrt(dx * dx + dz * dz);
            var c = dx / length;
            var s = dz / length;
            var extension = c * (displacement[j * 2] - displacement[i * 2]) +
                            s * (displacement[j * 2 + 1] - displacement[i * 2 + 1]);
            memberResults.Add(new(member.Id,
                EffectiveYoungsModulus(member, materialByTarget) *
                member.AreaM2 / length * extension));
        }

        return new LinearTrussResult
        {
            Nodes = nodeResults,
            Members = memberResults,
            MaximumDisplacementM = nodeResults.Max(x =>
                Math.Sqrt(x.DisplacementXM * x.DisplacementXM + x.DisplacementZM * x.DisplacementZM)),
            Warnings = warnings.Distinct(StringComparer.Ordinal).ToList()
        };
    }

    private static double EffectiveYoungsModulus(
        StructuralMember member,
        IReadOnlyDictionary<string, string> materialByTarget) =>
        materialByTarget.GetValueOrDefault(member.Id) switch
        {
            "steel" => 210_000_000_000,
            "concrete" => 30_000_000_000,
            _ => member.YoungsModulusPa
        };

    private static double[] SolveLinearSystem(double[,] matrix, double[] vector)
    {
        var n = vector.Length;
        var a = (double[,])matrix.Clone();
        var b = (double[])vector.Clone();

        for (var pivot = 0; pivot < n; pivot++)
        {
            var best = pivot;
            for (var row = pivot + 1; row < n; row++)
                if (Math.Abs(a[row, pivot]) > Math.Abs(a[best, pivot]))
                    best = row;
            if (Math.Abs(a[best, pivot]) < 1e-10)
                throw new InvalidOperationException(
                    "The model is unstable or under-constrained in the X-Z analysis plane.");

            if (best != pivot)
            {
                for (var column = pivot; column < n; column++)
                    (a[pivot, column], a[best, column]) = (a[best, column], a[pivot, column]);
                (b[pivot], b[best]) = (b[best], b[pivot]);
            }

            var divisor = a[pivot, pivot];
            for (var column = pivot; column < n; column++)
                a[pivot, column] /= divisor;
            b[pivot] /= divisor;

            for (var row = 0; row < n; row++)
            {
                if (row == pivot)
                    continue;
                var factor = a[row, pivot];
                if (Math.Abs(factor) < 1e-20)
                    continue;
                for (var column = pivot; column < n; column++)
                    a[row, column] -= factor * a[pivot, column];
                b[row] -= factor * b[pivot];
            }
        }
        return b;
    }

    private static double[] Multiply(double[,] matrix, double[] vector)
    {
        var result = new double[vector.Length];
        for (var row = 0; row < vector.Length; row++)
        for (var column = 0; column < vector.Length; column++)
            result[row] += matrix[row, column] * vector[column];
        return result;
    }
}
