using Microsoft.Win32;
using System.ComponentModel;
using Structural.Desktop.Core.Analysis;
using Structural.Desktop.Core.Commands;
using Structural.Desktop.Core.Jobs;
using Structural.Desktop.Core.Model;
using Structural.Desktop.Core.Palette;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace Structural.Desktop.Wpf;

public partial class MainWindow : Window
{
    private readonly WorkspaceState _workspace = new();
    private readonly CommandHistory _history = new();
    private readonly BackgroundJobManager _jobs = new();
    private readonly CommandPalette _palette = new();
    private readonly Dictionary<string, Point3D> _nodePositions = new(StringComparer.Ordinal);
    private readonly List<MemberGeometry> _members = [];
    private readonly Dictionary<GeometryModel3D, string> _visualTargets = [];

    private ModelNode? _selectedNode;
    private Point _resourceDragStart;
    private Point _cameraDragStart;
    private Point3D _cameraTarget = new(0, 0, 0);
    private double _cameraDistance = 10;
    private double _cameraYaw = Math.PI * 0.75;
    private double _cameraPitch = Math.PI * 0.30;
    private bool _orbiting;
    private bool _panning;
    private bool _selecting;
    private bool _cameraMoved;
    private bool _showGrid = true;
    private ViewportTool _viewportTool = ViewportTool.Select;
    private LinearTrussResult? _analysisResult;
    private double _deformationScale = 1;
    private string? _memberStartNodeId;

    public MainWindow()
    {
        InitializeComponent();
        SetViewportTool(ViewportTool.Select);
        LoadStarterModel();
        RegisterCommands();
        _history.Changed += (_, _) => Dispatcher.Invoke(() =>
        {
            _analysisResult = null;
            RefreshModelFromAuthority();
            RefreshState();
        });
        _jobs.Changed += (_, _) => Dispatcher.Invoke(RefreshJobs);
        RefreshState();
        RefreshJobs();
    }

    private void LoadStarterModel()
    {
        var model = StructuralModel.Starter();
        var root = BuildModelTree(model);
        ApplyModel(model);
        _workspace.SetDocument("built-in:starter-truss", root, model);
        _workspace.SetAssignment(new ResourceAssignment("node-a", ResourceKind.Support,
            "pinned", "Pinned support", DateTimeOffset.UtcNow));
        _workspace.SetAssignment(new ResourceAssignment("node-b", ResourceKind.Support,
            "roller", "Roller support", DateTimeOffset.UtcNow));
        _workspace.SetAssignment(new ResourceAssignment("node-c", ResourceKind.Load,
            "point-load", "Point load", DateTimeOffset.UtcNow, 25_000, 0, 0, -1));
        _workspace.MarkSaved("built-in:starter-truss");
        ModelTree.ItemsSource = new[] { root };
        BuildScene();
        FitCamera();
        StatusText.Text = "Starter truss loaded — orbit, edit the 25 kN load, then run analysis.";
    }

    private static ModelNode BuildModelTree(StructuralModel model)
    {
        var nodes = new ModelNode("nodes", $"Nodes ({model.Nodes.Count})", "collection",
            model.Nodes.Select(x => new ModelNode(x.Id, x.Name, "node")));
        var members = new ModelNode("members", $"Members ({model.Members.Count})", "collection",
            model.Members.Select(x => new ModelNode(x.Id, x.Name, "member")));
        return new ModelNode("root", model.Name, "document", [nodes, members]);
    }

    private void ApplyModel(StructuralModel model)
    {
        _nodePositions.Clear();
        foreach (var node in model.Nodes)
            _nodePositions[node.Id] = new Point3D(node.Position.X, node.Position.Y, node.Position.Z);
        _members.Clear();
        _members.AddRange(model.Members.Select(x =>
            new MemberGeometry(x.Id, x.StartNodeId, x.EndNodeId)));
        _analysisResult = null;
    }

    private void RegisterCommands()
    {
        _palette.Register(new("file.open", "Open model", "File", _ => OpenModelAsync()));
        _palette.Register(new("file.save-assignments", "Save assignments", "File",
            _ => SaveAssignmentsAsync()));
        _palette.Register(new("view.fit", "Fit model in view", "View", _ =>
        {
            FitCamera();
            return Task.CompletedTask;
        }, ["home", "zoom"]));
        _palette.Register(new("view.isometric", "Isometric view", "View", _ =>
        {
            SetView(Math.PI * 0.75, Math.PI * 0.30);
            return Task.CompletedTask;
        }, ["iso", "camera"]));
        _palette.Register(new("view.frame-selection", "Frame selected object", "View", _ =>
        {
            FrameSelection();
            return Task.CompletedTask;
        }, ["focus", "selection", "camera"]));
        _palette.Register(new("view.tool.select", "Use selection tool", "View", _ =>
        {
            SetViewportTool(ViewportTool.Select);
            return Task.CompletedTask;
        }, ["pick"]));
        _palette.Register(new("view.tool.orbit", "Use orbit tool", "View", _ =>
        {
            SetViewportTool(ViewportTool.Orbit);
            return Task.CompletedTask;
        }, ["rotate", "camera"]));
        _palette.Register(new("view.tool.pan", "Use pan tool", "View", _ =>
        {
            SetViewportTool(ViewportTool.Pan);
            return Task.CompletedTask;
        }, ["move", "camera"]));
        _palette.Register(new("edit.undo", "Undo", "Edit", _ =>
        {
            _history.Undo();
            return Task.CompletedTask;
        }));
        _palette.Register(new("edit.redo", "Redo", "Edit", _ =>
        {
            _history.Redo();
            return Task.CompletedTask;
        }));
        _palette.Register(new("jobs.test", "Run workspace tests", "Jobs", _ =>
        {
            StartWorkspaceTests();
            return Task.CompletedTask;
        }, ["cargo", "validate"]));
        _palette.Register(new("analysis.linear", "Run linear truss analysis", "Analysis", _ =>
        {
            RunAnalysis();
            return Task.CompletedTask;
        }, ["solve", "results", "fem"]));
    }

    private void New_Click(object sender, RoutedEventArgs e) => CreateNewModel();
    private async void Open_Click(object sender, RoutedEventArgs e) => await OpenModelAsync();
    private async void SaveModel_Click(object sender, RoutedEventArgs e) => await SaveModelAsync();
    private async void Save_Click(object sender, RoutedEventArgs e) => await SaveAssignmentsAsync();
    private void Undo_Click(object sender, RoutedEventArgs e) => _history.Undo();
    private void Redo_Click(object sender, RoutedEventArgs e) => _history.Redo();
    private void Validate_Click(object sender, RoutedEventArgs e) => ValidateModel();
    private void RunAnalysis_Click(object sender, RoutedEventArgs e) => RunAnalysis();
    private void SideView_Click(object sender, RoutedEventArgs e) => SetView(0, 0.05);
    private void Palette_Click(object sender, RoutedEventArgs e) => OpenPalette();
    private void HomeView_Click(object sender, RoutedEventArgs e) => FitCamera();
    private void FrameSelection_Click(object sender, RoutedEventArgs e) => FrameSelection();
    private void IsoView_Click(object sender, RoutedEventArgs e) => SetView(Math.PI * 0.75, Math.PI * 0.30);
    private void FrontView_Click(object sender, RoutedEventArgs e) => SetView(-Math.PI / 2, 0.05);
    private void TopView_Click(object sender, RoutedEventArgs e) => SetView(-Math.PI / 2, Math.PI / 2 - 0.01);
    private void SelectTool_Click(object sender, RoutedEventArgs e) => SetViewportTool(ViewportTool.Select);
    private void OrbitTool_Click(object sender, RoutedEventArgs e) => SetViewportTool(ViewportTool.Orbit);
    private void PanTool_Click(object sender, RoutedEventArgs e) => SetViewportTool(ViewportTool.Pan);
    private void Grid_Click(object sender, RoutedEventArgs e)
    {
        _showGrid = !_showGrid;
        UpdateViewportControls();
        BuildScene();
    }

    private void CreateNewModel()
    {
        if (!ConfirmDiscardChanges())
            return;
        var model = new StructuralModel("Untitled model",
            [new StructuralNode("node-1", "Node 1", new ModelPoint(0, 0, 0))],
            []);
        _history.Clear();
        _workspace.SetDocument("untitled", BuildModelTree(model), model);
        _selectedNode = null;
        _memberStartNodeId = null;
        RefreshModelFromAuthority();
        FitCamera();
        StatusText.Text = "New model created. Add nodes and members in the authoring panel.";
    }

    private bool ConfirmDiscardChanges()
    {
        if (!_workspace.IsDirty)
            return true;
        return MessageBox.Show(this,
            "The current model has unsaved changes. Continue without saving?",
            "Unsaved changes", MessageBoxButton.YesNo, MessageBoxImage.Warning) == MessageBoxResult.Yes;
    }

    private async Task SaveModelAsync()
    {
        var model = _workspace.Model;
        if (model is null)
            return;

        var existing = _workspace.SourcePath;
        string? path = existing is not null &&
                       !existing.StartsWith("built-in:", StringComparison.Ordinal) &&
                       !existing.Equals("untitled", StringComparison.Ordinal)
            ? existing
            : null;
        if (path is null)
        {
            var dialog = new SaveFileDialog
            {
                Filter = "Structural JSON (*.json)|*.json",
                FileName = "structural-model.json"
            };
            if (dialog.ShowDialog(this) != true)
                return;
            path = dialog.FileName;
        }

        await model.SaveAsync(path);
        _workspace.MarkSaved(path);
        UpdateWindowTitle();
        StatusText.Text = $"Saved model to {path}";
    }

    private void AddNode_Click(object sender, RoutedEventArgs e)
    {
        var model = _workspace.Model;
        if (model is null)
            return;
        var centre = model.Nodes.Count == 0
            ? new ModelPoint(0, 0, 0)
            : new ModelPoint(model.Nodes.Average(x => x.Position.X),
                model.Nodes.Average(x => x.Position.Y),
                model.Nodes.Average(x => x.Position.Z));
        var id = model.NextId("node");
        _history.Execute(new AddNodeCommand(model,
            new StructuralNode(id, $"Node {model.Nodes.Count + 1}", centre)));
        SelectById(id);
        StatusText.Text = $"Added {id}. Edit its coordinates in the properties panel.";
    }

    private void StartMember_Click(object sender, RoutedEventArgs e)
    {
        if (_selectedNode?.Kind != "node")
        {
            StatusText.Text = "Select the first node for the new member.";
            return;
        }
        _memberStartNodeId = _selectedNode.Id;
        AuthoringHintText.Text = $"Member start: {_selectedNode.Name}. Select another node and click Finish member.";
        StatusText.Text = $"Member start set to {_selectedNode.Name}.";
    }

    private void FinishMember_Click(object sender, RoutedEventArgs e)
    {
        var model = _workspace.Model;
        if (model is null || _memberStartNodeId is null)
        {
            StatusText.Text = "Choose Start member on the first node.";
            return;
        }
        if (_selectedNode?.Kind != "node" || _selectedNode.Id == _memberStartNodeId)
        {
            StatusText.Text = "Select a different end node.";
            return;
        }
        var id = model.NextId("member");
        _history.Execute(new AddMemberCommand(model,
            new StructuralMember(id, $"Member {model.Members.Count + 1}",
                _memberStartNodeId, _selectedNode.Id)));
        _memberStartNodeId = null;
        AuthoringHintText.Text = "Member created. Select nodes to create another member.";
        SelectById(id);
        StatusText.Text = $"Created {id}.";
    }

    private void DeleteEntity_Click(object sender, RoutedEventArgs e)
    {
        var model = _workspace.Model;
        if (model is null || _selectedNode is null)
            return;
        var id = _selectedNode.Id;
        if (_selectedNode.Kind == "member")
        {
            _history.Execute(new RemoveMemberCommand(model, id, _workspace));
        }
        else if (_selectedNode.Kind == "node")
        {
            var attached = model.Members.Count(x => x.StartNodeId == id || x.EndNodeId == id);
            if (attached > 0 && MessageBox.Show(this,
                    $"Deleting this node will also delete {attached} attached member(s). Continue?",
                    "Delete node", MessageBoxButton.YesNo, MessageBoxImage.Warning) != MessageBoxResult.Yes)
                return;
            _history.Execute(new RemoveNodeCommand(model, id, workspace: _workspace));
        }
        else
        {
            StatusText.Text = "Select a node or member to delete.";
            return;
        }

        _selectedNode = null;
        StatusText.Text = $"Deleted {id}.";
    }

    private void ApplyNodeCoordinates_Click(object sender, RoutedEventArgs e)
    {
        var model = _workspace.Model;
        if (model is null || _selectedNode?.Kind != "node")
            return;
        if (!double.TryParse(NodeXText.Text, out var x) ||
            !double.TryParse(NodeYText.Text, out var y) ||
            !double.TryParse(NodeZText.Text, out var z) ||
            !double.IsFinite(x) || !double.IsFinite(y) || !double.IsFinite(z))
        {
            MessageBox.Show(this, "Enter finite numeric X, Y and Z coordinates in metres.",
                "Invalid coordinates", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }
        var id = _selectedNode.Id;
        _history.Execute(new MoveNodeCommand(model, id, new ModelPoint(x, y, z)));
        SelectById(id);
        StatusText.Text = $"Updated coordinates for {id}.";
    }

    private void RefreshModelFromAuthority()
    {
        var model = _workspace.Model;
        if (model is null)
            return;
        var selectedId = _selectedNode?.Id;
        ApplyModel(model);
        var root = BuildModelTree(model);
        _workspace.UpdateModelTree(root);
        ModelTree.ItemsSource = new[] { root };
        _selectedNode = selectedId is null ? null : FindNode(root, selectedId);
        if (_selectedNode is not null)
            SelectTreeNode(_selectedNode);
        UpdateWindowTitle();
    }

    private void SelectById(string id)
    {
        var node = FindNode(_workspace.Root, id);
        if (node is null)
            return;
        _selectedNode = node;
        SelectTreeNode(node);
        UpdateSelectionText();
        BuildScene();
    }

    private void UpdateWindowTitle()
    {
        var modelName = _workspace.Model?.Name ?? "No model";
        Title = $"Structural Platform — {modelName}{(_workspace.IsDirty ? " *" : string.Empty)}";
    }

    private async Task OpenModelAsync()
    {
        if (!ConfirmDiscardChanges())
            return;
        var dialog = new OpenFileDialog
        {
            Filter = "Structural JSON (*.json)|*.json|All files (*.*)|*.*"
        };
        if (dialog.ShowDialog(this) != true)
            return;

        try
        {
            StatusText.Text = "Loading model…";
            var model = await StructuralModel.LoadAsync(dialog.FileName);
            var root = BuildModelTree(model);
            ApplyModel(model);
            _history.Clear();
            _workspace.SetDocument(dialog.FileName, root, model);
            UpdateWindowTitle();
            _selectedNode = null;
            ModelTree.ItemsSource = new[] { root };
            ModelTree.UpdateLayout();
            BuildScene();
            FitCamera();
            RefreshState();
            StatusText.Text = model.Diagnostics.Count == 0
                ? $"Loaded {Path.GetFileName(dialog.FileName)} — {model.Nodes.Count} nodes, {model.Members.Count} members"
                : $"Loaded with {model.Diagnostics.Count} validation message(s)";
        }
        catch (Exception exception)
        {
            MessageBox.Show(this, exception.Message, "Could not open model",
                MessageBoxButton.OK, MessageBoxImage.Error);
            StatusText.Text = "Load failed";
        }
    }

    private async Task SaveAssignmentsAsync()
    {
        var dialog = new SaveFileDialog
        {
            Filter = "JSON (*.json)|*.json",
            FileName = "desktop-assignments.json"
        };
        if (dialog.ShowDialog(this) != true)
            return;
        await _workspace.SaveAssignmentsAsync(dialog.FileName);
        StatusText.Text = $"Saved {dialog.FileName}";
    }

    private void ModelTree_SelectedItemChanged(object sender, RoutedPropertyChangedEventArgs<object> e)
    {
        if (e.NewValue is not ModelNode node)
            return;
        _selectedNode = node;
        UpdateSelectionText();
        BuildScene();
    }

    private void UpdateSelectionText()
    {
        if (_selectedNode is null)
        {
            SelectedName.Text = "Nothing selected";
            SelectedKind.Text = "Select an item in the tree or viewport.";
            ViewportLabel.Text = "3-D MODEL";
            NodeEditor.Visibility = Visibility.Collapsed;
            return;
        }

        SelectedName.Text = _selectedNode.Name;
        SelectedKind.Text = $"{_selectedNode.Kind} · {_selectedNode.Id}";
        ViewportLabel.Text = $"3-D MODEL · {_selectedNode.Name}";
        var structuralNode = _workspace.Model?.Nodes.FirstOrDefault(x => x.Id == _selectedNode.Id);
        NodeEditor.Visibility = structuralNode is null ? Visibility.Collapsed : Visibility.Visible;
        if (structuralNode is not null)
        {
            NodeXText.Text = structuralNode.Position.X.ToString("G10");
            NodeYText.Text = structuralNode.Position.Y.ToString("G10");
            NodeZText.Text = structuralNode.Position.Z.ToString("G10");
        }
    }

    private void Resource_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string encoded })
            AssignEncodedResource(_selectedNode, encoded);
    }

    private void Resource_PreviewMouseMove(object sender, MouseEventArgs e)
    {
        if (e.LeftButton != MouseButtonState.Pressed || sender is not Button button ||
            button.Tag is not string encoded)
            return;
        if ((e.GetPosition(button) - _resourceDragStart).Length <
            Math.Max(SystemParameters.MinimumHorizontalDragDistance,
                SystemParameters.MinimumVerticalDragDistance))
            return;
        DragDrop.DoDragDrop(button, encoded, DragDropEffects.Copy);
    }

    private void Resource_PreviewMouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (sender is Button button)
            _resourceDragStart = e.GetPosition(button);
    }

    private void ModelTree_DragOver(object sender, DragEventArgs e)
    {
        e.Effects = e.Data.GetDataPresent(DataFormats.StringFormat)
            ? DragDropEffects.Copy
            : DragDropEffects.None;
        e.Handled = true;
    }

    private void ModelTree_Drop(object sender, DragEventArgs e)
    {
        if (e.Data.GetData(DataFormats.StringFormat) is not string encoded)
            return;

        var item = FindAncestor<TreeViewItem>(e.OriginalSource as DependencyObject);
        var target = item?.DataContext as ModelNode ?? _selectedNode;
        AssignEncodedResource(target, encoded);
        e.Handled = true;
    }

    private void AssignEncodedResource(ModelNode? node, string encoded)
    {
        if (node is null)
        {
            StatusText.Text = "Select a node or member first.";
            MessageBox.Show(this, "Select a node or member first, then click the resource.",
                "No target selected", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        var parts = encoded.Split('|', 3);
        if (parts.Length != 3 || !Enum.TryParse<ResourceKind>(parts[0], out var kind))
            return;

        if (kind == ResourceKind.Support && !_nodePositions.ContainsKey(node.Id))
        {
            StatusText.Text = "Supports can only be assigned to a node.";
            MessageBox.Show(this, "Select a structural node before assigning a support.",
                "Invalid support target", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        if (kind == ResourceKind.Material && !_members.Any(x => x.Id == node.Id))
        {
            StatusText.Text = "Materials can only be assigned to a member.";
            MessageBox.Show(this, "Select a structural member before assigning a material.",
                "Invalid material target", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        if (kind == ResourceKind.Load && parts[1] == "point-load" &&
            !_nodePositions.ContainsKey(node.Id))
        {
            MessageBox.Show(this, "Point loads must be assigned to a node.",
                "Invalid load target", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        if (kind == ResourceKind.Load && parts[1] == "gravity" &&
            !_members.Any(x => x.Id == node.Id))
        {
            MessageBox.Show(this,
                "In this preview, gravity is represented as a total equivalent member load.",
                "Select a member", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        var payload = kind == ResourceKind.Load
            ? new ResourcePayload(kind, parts[1], parts[2], 10_000, 0, 0, -1)
            : new ResourcePayload(kind, parts[1], parts[2]);
        _analysisResult = null;
        _history.Execute(new AssignResourceCommand(_workspace, node.Id, payload));
        StatusText.Text = $"Assigned {parts[2]} to {node.Name}";
    }

    private void RemoveAssignment_Click(object sender, RoutedEventArgs e)
    {
        if (AssignmentsList.SelectedItem is not AssignmentListItem item)
        {
            StatusText.Text = "Select an assignment to remove.";
            return;
        }

        _analysisResult = null;
        _history.Execute(new RemoveResourceCommand(
            _workspace, item.Assignment.TargetId, item.Assignment.Kind));
        StatusText.Text = $"Removed {item.Assignment.DisplayName}.";
    }

    private void RefreshState()
    {
        UndoButton.IsEnabled = _history.CanUndo;
        RedoButton.IsEnabled = _history.CanRedo;
        UndoButton.ToolTip = _history.NextUndoDescription;
        RedoButton.ToolTip = _history.NextRedoDescription;
        AssignmentsList.ItemsSource = _workspace.Assignments
            .Select(x => new AssignmentListItem(x)).ToList();
        UpdateSelectionText();
        BuildScene();
    }

    private void ValidateModel()
    {
        var model = _workspace.Model;
        if (model is null)
        {
            StatusText.Text = "No structural model is loaded.";
            return;
        }

        ValidationList.ItemsSource = model.Diagnostics.Count == 0
            ? new[] { "✓ Model geometry and references are valid." }
            : model.Diagnostics.Select(x =>
                $"{x.Severity.ToUpperInvariant()} · {x.Code} · {x.Message}" +
                (x.TargetId is null ? string.Empty : $" [{x.TargetId}]")).ToList();
        ValidationExpander.IsExpanded = true;
        BottomTabs.SelectedIndex = 1;
        StatusText.Text = model.IsValid
            ? $"Validation complete — {model.Nodes.Count} nodes, {model.Members.Count} members"
            : "Validation found errors; analysis is blocked.";
    }

    private void RunAnalysis()
    {
        if (_workspace.Model is null)
        {
            StatusText.Text = "Open a supported structural model first.";
            return;
        }

        try
        {
            _analysisResult = LinearTrussSolver.Solve(_workspace.Model, _workspace.Assignments);
            _deformationScale = _analysisResult.MaximumDisplacementM > 1e-15
                ? Math.Min(ModelSize() * 0.18 / _analysisResult.MaximumDisplacementM, 5000)
                : 1;
            AnalysisList.ItemsSource = BuildAnalysisSummary(_analysisResult);
            BottomTabs.SelectedIndex = 0;
            BuildScene();
            StatusText.Text =
                $"Analysis complete — max displacement {_analysisResult.MaximumDisplacementM * 1000:G4} mm";
        }
        catch (Exception exception)
        {
            _analysisResult = null;
            BuildScene();
            MessageBox.Show(this, exception.Message, "Analysis could not run",
                MessageBoxButton.OK, MessageBoxImage.Warning);
            StatusText.Text = "Analysis failed — review supports, loads and model validation.";
        }
    }

    private static IReadOnlyList<string> BuildAnalysisSummary(LinearTrussResult result)
    {
        var lines = new List<string>
        {
            $"Maximum displacement: {result.MaximumDisplacementM * 1000:G5} mm",
            "Node displacements and reactions:"
        };
        lines.AddRange(result.Nodes.Select(x =>
            $"  {x.NodeId}: ux={x.DisplacementXM * 1000:G4} mm, uz={x.DisplacementZM * 1000:G4} mm" +
            $"  ·  Rx={x.ReactionXN / 1000:G4} kN, Rz={x.ReactionZN / 1000:G4} kN"));
        lines.Add("Member axial forces (+ tension):");
        lines.AddRange(result.Members.Select(x =>
            $"  {x.MemberId}: {x.AxialForceN / 1000:G5} kN"));
        lines.AddRange(result.Warnings.Select(x => $"Warning: {x}"));
        return lines;
    }

    private void AssignmentList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (AssignmentsList.SelectedItem is not AssignmentListItem item)
        {
            AssignmentEditor.Visibility = Visibility.Collapsed;
            return;
        }

        AssignmentEditor.Visibility = Visibility.Visible;
        AssignmentTargetText.Text = $"{item.Assignment.DisplayName} → {item.Assignment.TargetId}";
        LoadMagnitudeText.Text = (item.Assignment.MagnitudeN / 1000).ToString("G6");
        DirectionXText.Text = item.Assignment.DirectionX.ToString("G6");
        DirectionZText.Text = item.Assignment.DirectionZ.ToString("G6");
        LoadEditorPanel.Visibility = item.Assignment.Kind == ResourceKind.Load
            ? Visibility.Visible
            : Visibility.Collapsed;
    }

    private void ApplyAssignment_Click(object sender, RoutedEventArgs e)
    {
        if (AssignmentsList.SelectedItem is not AssignmentListItem item ||
            item.Assignment.Kind != ResourceKind.Load)
            return;

        if (!double.TryParse(LoadMagnitudeText.Text, out var magnitudeKn) ||
            !double.TryParse(DirectionXText.Text, out var directionX) ||
            !double.TryParse(DirectionZText.Text, out var directionZ) ||
            magnitudeKn <= 0 || Math.Abs(directionX) + Math.Abs(directionZ) < 1e-12)
        {
            MessageBox.Show(this,
                "Enter a positive load magnitude in kN and a non-zero X/Z direction.",
                "Invalid load", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        var updated = item.Assignment with
        {
            MagnitudeN = magnitudeKn * 1000,
            DirectionX = directionX,
            DirectionY = 0,
            DirectionZ = directionZ
        };
        _analysisResult = null;
        _history.Execute(new AssignResourceCommand(_workspace, updated));
        StatusText.Text = $"Updated load on {updated.TargetId}.";
    }

    private void AddDeformedShape(LinearTrussResult result)
    {
        var displaced = result.Nodes.ToDictionary(
            x => x.NodeId,
            x =>
            {
                var original = _nodePositions[x.NodeId];
                return new Point3D(
                    original.X + x.DisplacementXM * _deformationScale,
                    original.Y,
                    original.Z + x.DisplacementZM * _deformationScale);
            },
            StringComparer.Ordinal);

        foreach (var member in _members)
        {
            if (!displaced.TryGetValue(member.StartNodeId, out var start) ||
                !displaced.TryGetValue(member.EndNodeId, out var end))
                continue;
            var axial = result.Members.FirstOrDefault(x => x.MemberId == member.Id)?.AxialForceN ?? 0;
            var color = axial >= 0 ? Color.FromRgb(235, 92, 92) : Color.FromRgb(92, 148, 242);
            Scene.Children.Add(CreateBeam(start, end,
                Math.Max(ModelSize() * 0.012, 0.045), color));
        }

        foreach (var point in displaced.Values)
            Scene.Children.Add(CreateBox(point,
                Math.Max(ModelSize() * 0.022, 0.07),
                Math.Max(ModelSize() * 0.022, 0.07),
                Math.Max(ModelSize() * 0.022, 0.07),
                Color.FromRgb(250, 215, 90)));
    }

    private void StartWorkspaceTests()
    {
        var workspace = FindWorkspaceRoot();
        _jobs.StartProcess("Cargo workspace tests", "cargo", ["test", "--workspace"], workspace);
        StatusText.Text = "Workspace tests started";
    }

    private string FindWorkspaceRoot()
    {
        var current = new DirectoryInfo(AppContext.BaseDirectory);
        while (current is not null && !File.Exists(Path.Combine(current.FullName, "Cargo.toml")))
            current = current.Parent;
        return current?.FullName ?? Environment.CurrentDirectory;
    }

    private void RefreshJobs() => JobsGrid.ItemsSource = _jobs.Jobs;

    private void CancelJob_Click(object sender, RoutedEventArgs e)
    {
        if (JobsGrid.SelectedItem is BackgroundJob job)
            _jobs.Cancel(job.Id);
    }

    private void OpenPalette()
    {
        PaletteOverlay.Visibility = Visibility.Visible;
        PaletteQuery.Text = string.Empty;
        RefreshPalette();
        PaletteQuery.Focus();
    }

    private void ClosePalette() => PaletteOverlay.Visibility = Visibility.Collapsed;

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        if (!ConfirmDiscardChanges())
            e.Cancel = true;
    }

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        var isTextInput = e.OriginalSource is TextBox;
        if (e.Key == Key.K && Keyboard.Modifiers.HasFlag(ModifierKeys.Control))
        {
            OpenPalette();
            e.Handled = true;
        }
        else if (e.Key == Key.Escape && PaletteOverlay.Visibility == Visibility.Visible)
        {
            ClosePalette();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.S && Keyboard.Modifiers.HasFlag(ModifierKeys.Control))
        {
            _ = SaveModelAsync();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.Z && Keyboard.Modifiers.HasFlag(ModifierKeys.Control))
        {
            _history.Undo();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.Y && Keyboard.Modifiers.HasFlag(ModifierKeys.Control))
        {
            _history.Redo();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.Home)
        {
            FitCamera();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.F)
        {
            FrameSelection();
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.S)
        {
            SetViewportTool(ViewportTool.Select);
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.O)
        {
            SetViewportTool(ViewportTool.Orbit);
            e.Handled = true;
        }
        else if (!isTextInput && e.Key == Key.P)
        {
            SetViewportTool(ViewportTool.Pan);
            e.Handled = true;
        }
        else if (!isTextInput && (e.Key == Key.D1 || e.Key == Key.NumPad1))
        {
            SetView(-Math.PI / 2, 0.05);
            e.Handled = true;
        }
        else if (!isTextInput && (e.Key == Key.D2 || e.Key == Key.NumPad2))
        {
            SetView(0, 0.05);
            e.Handled = true;
        }
        else if (!isTextInput && (e.Key == Key.D3 || e.Key == Key.NumPad3))
        {
            SetView(-Math.PI / 2, Math.PI / 2 - 0.01);
            e.Handled = true;
        }
        else if (!isTextInput && (e.Key == Key.D4 || e.Key == Key.NumPad4))
        {
            SetView(Math.PI * 0.75, Math.PI * 0.30);
            e.Handled = true;
        }
    }

    private void PaletteQuery_TextChanged(object sender, TextChangedEventArgs e) => RefreshPalette();

    private void RefreshPalette()
    {
        PaletteResults.ItemsSource = _palette.Search(PaletteQuery.Text);
        if (PaletteResults.Items.Count > 0)
            PaletteResults.SelectedIndex = 0;
    }

    private async void PaletteQuery_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
            await RunSelectedPaletteCommandAsync();
    }

    private async void PaletteResults_MouseDoubleClick(object sender, MouseButtonEventArgs e) =>
        await RunSelectedPaletteCommandAsync();

    private async Task RunSelectedPaletteCommandAsync()
    {
        if (PaletteResults.SelectedItem is not PaletteCommand command)
            return;
        ClosePalette();
        await command.ExecuteAsync(CancellationToken.None);
    }

    private void Viewport_MouseDown(object sender, MouseButtonEventArgs e)
    {
        switch (e.ChangedButton)
        {
            case MouseButton.Left:
                Viewport_MouseLeftButtonDown(sender, e);
                break;
            case MouseButton.Middle:
                Viewport_MouseMiddleButtonDown(sender, e);
                break;
            case MouseButton.Right:
                Viewport_MouseRightButtonDown(sender, e);
                break;
        }
    }

    private void Viewport_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        Viewport.Focus();
        _cameraDragStart = e.GetPosition(Viewport);
        _cameraMoved = false;

        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt) || _viewportTool == ViewportTool.Orbit)
            _orbiting = true;
        else if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift) || _viewportTool == ViewportTool.Pan)
            _panning = true;
        else
            _selecting = true;

        Viewport.CaptureMouse();
        e.Handled = true;
    }

    private void Viewport_MouseMiddleButtonDown(object sender, MouseButtonEventArgs e)
    {
        Viewport.Focus();
        _cameraDragStart = e.GetPosition(Viewport);
        _cameraMoved = false;
        _orbiting = true;
        Viewport.CaptureMouse();
        e.Handled = true;
    }

    private void Viewport_MouseRightButtonDown(object sender, MouseButtonEventArgs e)
    {
        Viewport.Focus();
        _cameraDragStart = e.GetPosition(Viewport);
        _panning = true;
        _cameraMoved = false;
        Viewport.CaptureMouse();
        e.Handled = true;
    }

    private void Viewport_MouseMove(object sender, MouseEventArgs e)
    {
        var current = e.GetPosition(Viewport);
        var delta = current - _cameraDragStart;
        if (Math.Abs(delta.X) + Math.Abs(delta.Y) > 3)
            _cameraMoved = true;

        if (_orbiting &&
            (e.LeftButton == MouseButtonState.Pressed || e.MiddleButton == MouseButtonState.Pressed))
        {
            _cameraYaw -= delta.X * 0.008;
            _cameraPitch = Math.Clamp(_cameraPitch + delta.Y * 0.008,
                -Math.PI / 2 + 0.02, Math.PI / 2 - 0.02);
            UpdateCamera();
        }
        else if (_panning &&
                 (e.LeftButton == MouseButtonState.Pressed || e.RightButton == MouseButtonState.Pressed))
        {
            var look = Camera.LookDirection;
            look.Normalize();
            var right = Vector3D.CrossProduct(look, Camera.UpDirection);
            if (right.LengthSquared > 1e-12)
                right.Normalize();
            var up = Vector3D.CrossProduct(right, look);
            if (up.LengthSquared > 1e-12)
                up.Normalize();
            var scale = _cameraDistance * 0.0018;
            _cameraTarget += right * (-delta.X * scale) + up * (delta.Y * scale);
            UpdateCamera();
        }

        _cameraDragStart = current;
    }

    private void Viewport_MouseButtonUp(object sender, MouseButtonEventArgs e)
    {
        if (_selecting && !_cameraMoved && e.ChangedButton == MouseButton.Left)
        {
            SelectViewportObject(e.GetPosition(Viewport));
            if (e.ClickCount == 2)
                FrameSelection();
        }

        _orbiting = false;
        _panning = false;
        _selecting = false;
        Viewport.ReleaseMouseCapture();
        e.Handled = true;
    }

    private void Viewport_MouseLeave(object sender, MouseEventArgs e)
    {
        if (e.LeftButton == MouseButtonState.Released &&
            e.MiddleButton == MouseButtonState.Released &&
            e.RightButton == MouseButtonState.Released)
        {
            _orbiting = false;
            _panning = false;
            _selecting = false;
            Viewport.ReleaseMouseCapture();
        }
    }

    private void Viewport_MouseWheel(object sender, MouseWheelEventArgs e)
    {
        _cameraDistance *= e.Delta > 0 ? 0.86 : 1.16;
        var minimum = Math.Max(ModelSize() * 0.02, 0.05);
        var maximum = Math.Max(ModelSize() * 1000, 100);
        _cameraDistance = Math.Clamp(_cameraDistance, minimum, maximum);
        UpdateCamera();
        e.Handled = true;
    }

    private void SelectViewportObject(Point point)
    {
        var result = VisualTreeHelper.HitTest(Viewport, point);
        if (result is not RayMeshGeometry3DHitTestResult hit ||
            hit.ModelHit is not GeometryModel3D model ||
            !_visualTargets.TryGetValue(model, out var id))
            return;

        var node = FindNode(_workspace.Root, id);
        if (node is null)
            return;

        _selectedNode = node;
        SelectTreeNode(node);
        UpdateSelectionText();
        BuildScene();
        StatusText.Text = $"Selected {node.Name}";
    }

    private void SelectTreeNode(ModelNode node)
    {
        ModelTree.UpdateLayout();
        foreach (var item in ModelTree.Items)
        {
            if (SelectTreeNode(ModelTree, item, node))
                break;
        }
    }

    private static bool SelectTreeNode(ItemsControl parent, object item, ModelNode target)
    {
        if (parent.ItemContainerGenerator.ContainerFromItem(item) is not TreeViewItem container)
            return false;

        if (ReferenceEquals(item, target))
        {
            container.IsSelected = true;
            container.BringIntoView();
            return true;
        }

        container.IsExpanded = true;
        container.UpdateLayout();
        foreach (var child in container.Items)
        {
            if (SelectTreeNode(container, child, target))
                return true;
        }
        return false;
    }

    private void BuildScene()
    {
        if (Scene is null)
            return;

        Scene.Children.Clear();
        _visualTargets.Clear();
        Scene.Children.Add(new AmbientLight(Color.FromRgb(85, 92, 104)));
        Scene.Children.Add(new DirectionalLight(Color.FromRgb(245, 248, 255), new Vector3D(-1, -1, -2)));
        Scene.Children.Add(new DirectionalLight(Color.FromRgb(80, 100, 120), new Vector3D(1, 1, -0.5)));
        if (_showGrid)
            AddGrid();
        AddWorldAxes();

        foreach (var member in _members)
        {
            if (!_nodePositions.TryGetValue(member.StartNodeId, out var start) ||
                !_nodePositions.TryGetValue(member.EndNodeId, out var end))
                continue;

            var color = GetMemberColor(member.Id);
            var thickness = Math.Max(ModelSize() * 0.025, 0.10);
            var visual = CreateBeam(start, end, thickness, color);
            Scene.Children.Add(visual);
            _visualTargets[visual] = member.Id;
        }

        var nodeRadius = Math.Max(ModelSize() * 0.035, 0.12);
        foreach (var (id, position) in _nodePositions)
        {
            var selected = _selectedNode?.Id == id;
            var node = CreateBox(position, nodeRadius * 1.25, nodeRadius * 1.25, nodeRadius * 1.25,
                selected ? Color.FromRgb(255, 196, 75) : Color.FromRgb(85, 205, 198));
            Scene.Children.Add(node);
            _visualTargets[node] = id;
        }

        foreach (var assignment in _workspace.Assignments)
            AddAssignmentVisual(assignment);

        if (_analysisResult is not null)
            AddDeformedShape(_analysisResult);
    }

    private Color GetMemberColor(string memberId)
    {
        if (_selectedNode?.Id == memberId)
            return Color.FromRgb(255, 196, 75);

        var material = _workspace.GetAssignment(memberId, ResourceKind.Material);
        return material?.ResourceId switch
        {
            "concrete" => Color.FromRgb(160, 165, 172),
            "steel" => Color.FromRgb(74, 178, 190),
            _ => Color.FromRgb(54, 145, 154)
        };
    }

    private void AddAssignmentVisual(ResourceAssignment assignment)
    {
        if (assignment.Kind == ResourceKind.Support &&
            _nodePositions.TryGetValue(assignment.TargetId, out var node))
        {
            var size = Math.Max(ModelSize() * 0.11, 0.35);
            if (assignment.ResourceId == "fixed")
            {
                Scene.Children.Add(CreateBox(
                    new Point3D(node.X, node.Y, node.Z - size * 0.55),
                    size, size, size * 0.22, Color.FromRgb(235, 106, 82)));
            }
            else if (assignment.ResourceId == "roller")
            {
                Scene.Children.Add(CreatePyramid(
                    new Point3D(node.X, node.Y, node.Z - size * 0.08),
                    size, size * 0.72, Color.FromRgb(92, 176, 238)));
                Scene.Children.Add(CreateBox(
                    new Point3D(node.X - size * 0.23, node.Y, node.Z - size * 0.90),
                    size * 0.18, size * 0.32, size * 0.18, Color.FromRgb(92, 176, 238)));
                Scene.Children.Add(CreateBox(
                    new Point3D(node.X + size * 0.23, node.Y, node.Z - size * 0.90),
                    size * 0.18, size * 0.32, size * 0.18, Color.FromRgb(92, 176, 238)));
            }
            else
            {
                Scene.Children.Add(CreatePyramid(
                    new Point3D(node.X, node.Y, node.Z - size * 0.10),
                    size, size * 0.85, Color.FromRgb(239, 152, 67)));
            }
        }
        else if (assignment.Kind == ResourceKind.Load)
        {
            if (_nodePositions.TryGetValue(assignment.TargetId, out var point))
                AddDownArrow(point, Math.Max(ModelSize() * 0.45, 1.0));
            else
            {
                var member = _members.FirstOrDefault(x => x.Id == assignment.TargetId);
                if (member is not null &&
                    _nodePositions.TryGetValue(member.StartNodeId, out var a) &&
                    _nodePositions.TryGetValue(member.EndNodeId, out var b))
                    AddDownArrow(Midpoint(a, b), Math.Max(ModelSize() * 0.45, 1.0));
            }
        }
    }

    private void AddDownArrow(Point3D target, double length)
    {
        var red = Color.FromRgb(242, 91, 91);
        var top = new Point3D(target.X, target.Y, target.Z + length);
        var shaftEnd = new Point3D(target.X, target.Y, target.Z + length * 0.18);
        Scene.Children.Add(CreateBeam(top, shaftEnd, length * 0.055, red));
        Scene.Children.Add(CreatePyramid(
            new Point3D(target.X, target.Y, target.Z + length * 0.28),
            length * 0.28, length * 0.30, red, invert: true));
    }

    private void AddWorldAxes()
    {
        if (_nodePositions.Count == 0)
            return;

        var size = Math.Max(ModelSize() * 0.18, 0.75);
        var width = Math.Max(ModelSize() * 0.007, 0.025);
        var minX = _nodePositions.Values.Min(x => x.X);
        var minY = _nodePositions.Values.Min(x => x.Y);
        var minZ = _nodePositions.Values.Min(x => x.Z);
        var origin = new Point3D(minX - size * 0.35, minY - size * 0.35, minZ);
        Scene.Children.Add(CreateBeam(origin, origin + new Vector3D(size, 0, 0), width,
            Color.FromRgb(220, 75, 75)));
        Scene.Children.Add(CreateBeam(origin, origin + new Vector3D(0, size, 0), width,
            Color.FromRgb(78, 190, 105)));
        Scene.Children.Add(CreateBeam(origin, origin + new Vector3D(0, 0, size), width,
            Color.FromRgb(80, 135, 235)));
    }

    private void AddGrid()
    {
        if (_nodePositions.Count == 0)
            return;

        var minX = _nodePositions.Values.Min(x => x.X);
        var maxX = _nodePositions.Values.Max(x => x.X);
        var minY = _nodePositions.Values.Min(x => x.Y);
        var maxY = _nodePositions.Values.Max(x => x.Y);
        var minZ = _nodePositions.Values.Min(x => x.Z);
        var span = Math.Max(Math.Max(maxX - minX, maxY - minY), 1);
        var step = NiceStep(span / 10);
        var extent = Math.Ceiling(span / step / 2 + 2) * step;
        var cx = (minX + maxX) / 2;
        var cy = (minY + maxY) / 2;
        var z = minZ - Math.Max(span * 0.08, 0.25);
        var lineWidth = Math.Max(span * 0.0015, 0.006);
        var gridColor = Color.FromRgb(39, 48, 58);

        for (var offset = -extent; offset <= extent + step * 0.1; offset += step)
        {
            Scene.Children.Add(CreateBox(new Point3D(cx + offset, cy, z),
                lineWidth, extent * 2, lineWidth, gridColor));
            Scene.Children.Add(CreateBox(new Point3D(cx, cy + offset, z),
                extent * 2, lineWidth, lineWidth, gridColor));
        }

        Scene.Children.Add(CreateBox(new Point3D(cx, cy, z), extent * 2, lineWidth * 2, lineWidth * 2,
            Color.FromRgb(180, 70, 70)));
        Scene.Children.Add(CreateBox(new Point3D(cx, cy, z), lineWidth * 2, extent * 2, lineWidth * 2,
            Color.FromRgb(70, 150, 95)));
    }

    private void FitCamera()
    {
        if (_nodePositions.Count == 0)
        {
            _cameraTarget = new Point3D(0, 0, 0);
            _cameraDistance = 10;
        }
        else
        {
            var minX = _nodePositions.Values.Min(x => x.X);
            var maxX = _nodePositions.Values.Max(x => x.X);
            var minY = _nodePositions.Values.Min(x => x.Y);
            var maxY = _nodePositions.Values.Max(x => x.Y);
            var minZ = _nodePositions.Values.Min(x => x.Z);
            var maxZ = _nodePositions.Values.Max(x => x.Z);
            _cameraTarget = new Point3D((minX + maxX) / 2, (minY + maxY) / 2, (minZ + maxZ) / 2);
            var diagonal = new Vector3D(maxX - minX, maxY - minY, maxZ - minZ).Length;
            _cameraDistance = Math.Max(diagonal * 1.9, 4);
        }
        _cameraYaw = Math.PI * 0.75;
        _cameraPitch = Math.PI * 0.30;
        UpdateCamera();
    }

    private void SetView(double yaw, double pitch)
    {
        _cameraYaw = yaw;
        _cameraPitch = pitch;
        UpdateCamera();
    }

    private void UpdateCamera()
    {
        var horizontal = _cameraDistance * Math.Cos(_cameraPitch);
        var offset = new Vector3D(
            horizontal * Math.Cos(_cameraYaw),
            horizontal * Math.Sin(_cameraYaw),
            _cameraDistance * Math.Sin(_cameraPitch));
        Camera.Position = _cameraTarget + offset;
        Camera.LookDirection = _cameraTarget - Camera.Position;
        Camera.UpDirection = new Vector3D(0, 0, 1);
        Camera.NearPlaneDistance = Math.Max(_cameraDistance / 10000, 0.001);
        Camera.FarPlaneDistance = Math.Max(_cameraDistance * 100, 1000);
        UpdateCameraStatus();
    }

    private void SetViewportTool(ViewportTool tool)
    {
        _viewportTool = tool;
        Viewport.Cursor = tool switch
        {
            ViewportTool.Orbit => Cursors.SizeAll,
            ViewportTool.Pan => Cursors.Hand,
            _ => Cursors.Arrow
        };
        UpdateViewportControls();
        StatusText.Text = $"{tool} tool active";
    }

    private void UpdateViewportControls()
    {
        if (SelectToolButton is null)
            return;

        var active = new SolidColorBrush(Color.FromRgb(35, 115, 111));
        var normal = new SolidColorBrush(Color.FromRgb(39, 48, 58));
        SelectToolButton.Background = _viewportTool == ViewportTool.Select ? active : normal;
        OrbitToolButton.Background = _viewportTool == ViewportTool.Orbit ? active : normal;
        PanToolButton.Background = _viewportTool == ViewportTool.Pan ? active : normal;
        GridButton.Background = _showGrid ? active : normal;
        ViewportHintText.Text = _viewportTool switch
        {
            ViewportTool.Orbit => "Orbit tool · Left or middle-drag · Right-drag to pan · Wheel to zoom · S returns to select",
            ViewportTool.Pan => "Pan tool · Left or right-drag · Middle-drag to orbit · Wheel to zoom · S returns to select",
            _ => "Select tool · Click an object · Double-click to frame · Alt+left/middle orbit · Shift+left/right pan"
        };
        UpdateCameraStatus();
    }

    private void UpdateCameraStatus()
    {
        if (CameraStatusText is null)
            return;
        CameraStatusText.Text =
            $"{_viewportTool} · distance {_cameraDistance:G4} m · grid {(_showGrid ? "on" : "off")}";
    }

    private void FrameSelection()
    {
        if (_selectedNode is null)
        {
            FitCamera();
            return;
        }

        if (_nodePositions.TryGetValue(_selectedNode.Id, out var point))
        {
            _cameraTarget = point;
            _cameraDistance = Math.Max(ModelSize() * 0.55, 1.5);
        }
        else
        {
            var member = _members.FirstOrDefault(x => x.Id == _selectedNode.Id);
            if (member is null ||
                !_nodePositions.TryGetValue(member.StartNodeId, out var start) ||
                !_nodePositions.TryGetValue(member.EndNodeId, out var end))
            {
                FitCamera();
                return;
            }

            _cameraTarget = Midpoint(start, end);
            _cameraDistance = Math.Max((end - start).Length * 1.8, ModelSize() * 0.35);
        }

        UpdateCamera();
        StatusText.Text = $"Framed {_selectedNode.Name}";
    }

    private double ModelSize()
    {
        if (_nodePositions.Count < 2)
            return 4;
        var xs = _nodePositions.Values.Select(x => x.X).ToArray();
        var ys = _nodePositions.Values.Select(x => x.Y).ToArray();
        var zs = _nodePositions.Values.Select(x => x.Z).ToArray();
        return Math.Max(new Vector3D(xs.Max() - xs.Min(), ys.Max() - ys.Min(), zs.Max() - zs.Min()).Length, 1);
    }

    private static GeometryModel3D CreateBeam(Point3D start, Point3D end, double thickness, Color color)
    {
        var direction = end - start;
        var length = direction.Length;
        if (length < 1e-9)
            return CreateBox(start, thickness, thickness, thickness, color);

        direction.Normalize();
        var xAxis = new Vector3D(1, 0, 0);
        var axis = Vector3D.CrossProduct(xAxis, direction);
        var dot = Math.Clamp(Vector3D.DotProduct(xAxis, direction), -1, 1);
        var angle = Math.Acos(dot) * 180 / Math.PI;

        var transforms = new Transform3DGroup();
        transforms.Children.Add(new ScaleTransform3D(length, thickness, thickness));
        if (axis.LengthSquared > 1e-12)
        {
            axis.Normalize();
            transforms.Children.Add(new RotateTransform3D(
                new AxisAngleRotation3D(axis, angle)));
        }
        else if (dot < 0)
        {
            transforms.Children.Add(new RotateTransform3D(
                new AxisAngleRotation3D(new Vector3D(0, 0, 1), 180)));
        }

        var midpoint = Midpoint(start, end);
        transforms.Children.Add(new TranslateTransform3D(midpoint.X, midpoint.Y, midpoint.Z));
        return CreateGeometry(CreateUnitBoxMesh(), color, transforms);
    }

    private static GeometryModel3D CreateBox(
        Point3D center, double sizeX, double sizeY, double sizeZ, Color color)
    {
        var transforms = new Transform3DGroup();
        transforms.Children.Add(new ScaleTransform3D(sizeX, sizeY, sizeZ));
        transforms.Children.Add(new TranslateTransform3D(center.X, center.Y, center.Z));
        return CreateGeometry(CreateUnitBoxMesh(), color, transforms);
    }

    private static GeometryModel3D CreatePyramid(
        Point3D top, double width, double height, Color color, bool invert = false)
    {
        var half = width / 2;
        var zBase = invert ? top.Z + height : top.Z - height;
        var mesh = new MeshGeometry3D
        {
            Positions = new Point3DCollection
            {
                top,
                new(top.X-half, top.Y-half, zBase),
                new(top.X+half, top.Y-half, zBase),
                new(top.X+half, top.Y+half, zBase),
                new(top.X-half, top.Y+half, zBase)
            },
            TriangleIndices = new Int32Collection
            {
                0,1,2, 0,2,3, 0,3,4, 0,4,1,
                1,4,3, 1,3,2
            }
        };
        return CreateGeometry(mesh, color);
    }

    private static GeometryModel3D CreateGeometry(
        MeshGeometry3D mesh, Color color, Transform3D? transform = null)
    {
        var brush = new SolidColorBrush(color);
        brush.Freeze();
        var material = new MaterialGroup();
        material.Children.Add(new DiffuseMaterial(brush));
        material.Children.Add(new SpecularMaterial(
            new SolidColorBrush(Color.FromArgb(110, 255, 255, 255)), 28));
        material.Freeze();
        return new GeometryModel3D(mesh, material)
        {
            BackMaterial = material,
            Transform = transform ?? Transform3D.Identity
        };
    }

    private static MeshGeometry3D CreateUnitBoxMesh() => new()
    {
        Positions = new Point3DCollection
        {
            new(-0.5,-0.5,-0.5), new(0.5,-0.5,-0.5), new(0.5,0.5,-0.5), new(-0.5,0.5,-0.5),
            new(-0.5,-0.5,0.5), new(0.5,-0.5,0.5), new(0.5,0.5,0.5), new(-0.5,0.5,0.5)
        },
        TriangleIndices = new Int32Collection
        {
            0,2,1, 0,3,2, 4,5,6, 4,6,7,
            0,1,5, 0,5,4, 2,3,7, 2,7,6,
            0,4,7, 0,7,3, 1,2,6, 1,6,5
        }
    };

    private static Point3D Midpoint(Point3D a, Point3D b) =>
        new((a.X + b.X) / 2, (a.Y + b.Y) / 2, (a.Z + b.Z) / 2);

    private static double NiceStep(double value)
    {
        if (value <= 0)
            return 1;
        var exponent = Math.Pow(10, Math.Floor(Math.Log10(value)));
        var fraction = value / exponent;
        var nice = fraction < 2 ? 1 : fraction < 5 ? 2 : 5;
        return nice * exponent;
    }

    private static ModelNode? FindNode(ModelNode? node, string id)
    {
        if (node is null)
            return null;
        if (node.Id == id)
            return node;
        foreach (var child in node.Children)
        {
            var found = FindNode(child, id);
            if (found is not null)
                return found;
        }
        return null;
    }

    private static T? FindAncestor<T>(DependencyObject? source) where T : DependencyObject
    {
        while (source is not null)
        {
            if (source is T match)
                return match;
            source = VisualTreeHelper.GetParent(source);
        }
        return null;
    }

    private enum ViewportTool
    {
        Select,
        Orbit,
        Pan
    }

    private sealed record MemberGeometry(string Id, string StartNodeId, string EndNodeId);

    private sealed record AssignmentListItem(ResourceAssignment Assignment)
    {
        public override string ToString()
        {
            var value = Assignment.Kind == ResourceKind.Load
                ? $" · {Assignment.MagnitudeN / 1000:G5} kN"
                : string.Empty;
            return $"{Assignment.TargetId}  ·  {Assignment.Kind}: {Assignment.DisplayName}{value}";
        }
    }
}
