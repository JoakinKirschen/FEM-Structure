namespace Structural.Desktop.Core.Model;

public sealed class ModelNode
{
    public ModelNode(string id, string name, string kind, IEnumerable<ModelNode>? children = null)
    {
        Id = id;
        Name = name;
        Kind = kind;
        Children = children?.ToList() ?? [];
    }

    public string Id { get; }
    public string Name { get; }
    public string Kind { get; }
    public IList<ModelNode> Children { get; }
    public override string ToString() => Name;
}
