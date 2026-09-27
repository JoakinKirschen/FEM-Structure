using Structural.Desktop.Core.Analysis;
using Structural.Desktop.Core.Commands;
using Structural.Desktop.Core.Model;
using Structural.Desktop.Core.Palette;

var tests = new (string Name, Func<Task> Run)[]
{
    ("assignment undo/redo", () => RunSync(AssignmentUndoRedo)),
    ("replacement undo", () => RunSync(ReplacementUndo)),
    ("palette deterministic ordering", () => RunSync(PaletteOrdering)),
    ("duplicate palette command rejected", () => RunSync(DuplicateCommandRejected)),
    ("document tree loading", DocumentTreeLoading),
    ("structural model parsing", () => RunSync(StructuralModelParsing)),
    ("dangling member validation", () => RunSync(DanglingMemberValidation)),
    ("remove assignment undo", () => RunSync(RemoveAssignmentUndo)),
    ("linear truss analysis", () => RunSync(LinearTrussAnalysis)),
    ("model authoring undo redo", () => RunSync(ModelAuthoringUndoRedo)),
    ("cascade node delete undo", () => RunSync(CascadeNodeDeleteUndo)),
    ("model save round trip", ModelSaveRoundTrip)
};

var failures = new List<string>();
foreach (var test in tests)
{
    try
    {
        await test.Run();
        Console.WriteLine($"PASS {test.Name}");
    }
    catch (Exception exception)
    {
        failures.Add($"{test.Name}: {exception.Message}");
        Console.Error.WriteLine($"FAIL {test.Name}: {exception}");
    }
}
return failures.Count == 0 ? 0 : 1;

static Task RunSync(Action action)
{
    action();
    return Task.CompletedTask;
}

static void AssignmentUndoRedo()
{
    var state = new WorkspaceState();
    var history = new CommandHistory();
    history.Execute(new AssignResourceCommand(state, "member-1",
        new ResourcePayload(ResourceKind.Material, "steel", "Steel")));
    Equal("steel", state.GetAssignment("member-1", ResourceKind.Material)?.ResourceId);
    history.Undo();
    Equal<ResourceAssignment?>(null, state.GetAssignment("member-1", ResourceKind.Material));
    history.Redo();
    Equal("steel", state.GetAssignment("member-1", ResourceKind.Material)?.ResourceId);
}

static void ReplacementUndo()
{
    var state = new WorkspaceState();
    state.SetAssignment(new ResourceAssignment("node-1", ResourceKind.Support,
        "pinned", "Pinned", DateTimeOffset.UnixEpoch));
    var history = new CommandHistory();
    history.Execute(new AssignResourceCommand(state, "node-1",
        new ResourcePayload(ResourceKind.Support, "fixed", "Fixed")));
    history.Undo();
    Equal("pinned", state.GetAssignment("node-1", ResourceKind.Support)?.ResourceId);
}

static void PaletteOrdering()
{
    var palette = new CommandPalette();
    palette.Register(new("b", "Run validation", "Jobs", _ => Task.CompletedTask));
    palette.Register(new("a", "Run analysis", "Jobs", _ => Task.CompletedTask));
    var results = palette.Search("run");
    Equal("Run analysis", results[0].Title);
    Equal("Run validation", results[1].Title);
}

static void DuplicateCommandRejected()
{
    var palette = new CommandPalette();
    palette.Register(new("same", "One", "Test", _ => Task.CompletedTask));
    try
    {
        palette.Register(new("same", "Two", "Test", _ => Task.CompletedTask));
        throw new Exception("Expected duplicate command registration to fail.");
    }
    catch (InvalidOperationException)
    {
    }
}

static async Task DocumentTreeLoading()
{
    var path = Path.Combine(Path.GetTempPath(), $"structural-desktop-{Guid.NewGuid():N}.json");
    try
    {
        await File.WriteAllTextAsync(path,
            """{"name":"Demo","members":[{"id":"m-2","name":"Second"},{"id":"m-1","name":"First"}]}""");
        var root = await DocumentLoader.LoadAsync(path);
        Equal("Demo", root.Name);
        var members = root.Children.Single(x => x.Name.StartsWith("Members", StringComparison.Ordinal));
        Equal("Members (2)", members.Name);
        Equal("m-2", members.Children[0].Id);
    }
    finally
    {
        File.Delete(path);
    }
}

static void StructuralModelParsing()
{
    using var document = System.Text.Json.JsonDocument.Parse(
        """{"name":"Frame","nodes":[{"id":"a","xyz_m":[0,0,0]},{"id":"b","position":{"x":2,"y":0,"z":1}}],"members":[{"id":"m","start_node_id":"a","end_node_id":"b","area_m2":0.02}]}""");
    var model = StructuralModel.Parse(document.RootElement);
    Equal(true, model.IsValid);
    Equal(2, model.Nodes.Count);
    Equal(1, model.Members.Count);
    Equal(0.02, model.Members[0].AreaM2);
}

static void DanglingMemberValidation()
{
    using var document = System.Text.Json.JsonDocument.Parse(
        """{"nodes":[{"id":"a","xyz_m":[0,0,0]}],"members":[{"id":"m","start_node_id":"a","end_node_id":"missing"}]}""");
    var model = StructuralModel.Parse(document.RootElement);
    Equal(false, model.IsValid);
    Equal(true, model.Diagnostics.Any(x => x.Code == "member.node.dangling"));
}

static void RemoveAssignmentUndo()
{
    var state = new WorkspaceState();
    state.SetAssignment(new ResourceAssignment("n", ResourceKind.Support,
        "pinned", "Pinned", DateTimeOffset.UnixEpoch));
    var history = new CommandHistory();
    history.Execute(new RemoveResourceCommand(state, "n", ResourceKind.Support));
    Equal<ResourceAssignment?>(null, state.GetAssignment("n", ResourceKind.Support));
    history.Undo();
    Equal("pinned", state.GetAssignment("n", ResourceKind.Support)?.ResourceId);
}

static void LinearTrussAnalysis()
{
    var model = StructuralModel.Starter();
    var assignments = new ResourceAssignment[]
    {
        new("node-a", ResourceKind.Support, "pinned", "Pinned", DateTimeOffset.UnixEpoch),
        new("node-b", ResourceKind.Support, "roller", "Roller", DateTimeOffset.UnixEpoch),
        new("node-c", ResourceKind.Load, "point-load", "Point", DateTimeOffset.UnixEpoch,
            25_000, 0, 0, -1)
    };
    var result = LinearTrussSolver.Solve(model, assignments);
    Equal(true, result.MaximumDisplacementM > 0);
    Equal(true, result.Nodes.Single(x => x.NodeId == "node-c").DisplacementZM < 0);
    var verticalReaction = result.Nodes.Sum(x => x.ReactionZN);
    Equal(true, Math.Abs(verticalReaction - 25_000) < 0.1);
}

static void ModelAuthoringUndoRedo()
{
    var model = new StructuralModel("Edit", [
        new StructuralNode("a", "A", new ModelPoint(0, 0, 0))
    ], []);
    var history = new CommandHistory();
    history.Execute(new AddNodeCommand(model,
        new StructuralNode("b", "B", new ModelPoint(2, 0, 0))));
    history.Execute(new AddMemberCommand(model,
        new StructuralMember("m", "M", "a", "b")));
    history.Execute(new MoveNodeCommand(model, "b", new ModelPoint(3, 0, 1)));
    Equal(new ModelPoint(3, 0, 1), model.Nodes.Single(x => x.Id == "b").Position);
    history.Undo();
    Equal(new ModelPoint(2, 0, 0), model.Nodes.Single(x => x.Id == "b").Position);
    history.Undo();
    Equal(0, model.Members.Count);
    history.Redo();
    Equal(1, model.Members.Count);
}

static void CascadeNodeDeleteUndo()
{
    var model = StructuralModel.Starter();
    var workspace = new WorkspaceState();
    workspace.SetAssignment(new ResourceAssignment("member-1", ResourceKind.Material,
        "steel", "Steel", DateTimeOffset.UnixEpoch));
    var history = new CommandHistory();
    history.Execute(new RemoveNodeCommand(model, "node-c", workspace: workspace));
    Equal(2, model.Nodes.Count);
    Equal(1, model.Members.Count);
    Equal<ResourceAssignment?>(null,
        workspace.GetAssignment("member-1", ResourceKind.Material));
    history.Undo();
    Equal(3, model.Nodes.Count);
    Equal(3, model.Members.Count);
    Equal("steel", workspace.GetAssignment("member-1", ResourceKind.Material)?.ResourceId);
}

static async Task ModelSaveRoundTrip()
{
    var path = Path.Combine(Path.GetTempPath(), $"structural-model-{Guid.NewGuid():N}.json");
    try
    {
        var model = StructuralModel.Starter();
        model.ReplaceNode(model.Nodes.Single(x => x.Id == "node-c") with
        {
            Position = new ModelPoint(3, 0.5, 3)
        });
        await model.SaveAsync(path);
        var loaded = await StructuralModel.LoadAsync(path);
        Equal(true, loaded.IsValid);
        Equal(model.Nodes.Count, loaded.Nodes.Count);
        Equal(new ModelPoint(3, 0.5, 3),
            loaded.Nodes.Single(x => x.Id == "node-c").Position);
    }
    finally
    {
        File.Delete(path);
    }
}

static void Equal<T>(T expected, T actual)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
        throw new Exception($"Expected '{expected}', got '{actual}'.");
}
