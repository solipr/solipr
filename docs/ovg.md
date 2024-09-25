# Ordered Value Graph

[SVGs](svg.md) are useful for handling a single value, but the problem is that they don't allow handling multiple ordered values.

For this reason, we created a new data structure that we called the Ordered Value Graph (OVG).

This data structure is a [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph) where each node corresponds to a value.

Each node contains four pieces of information, here are the first two:
- The value contained in the node (represented by the text inside the node in the examples)
- A link to the next node (also called the child of the node, represented by the green arrows going from the node to the next node in the examples)

We will discuss the other two pieces of information in the section on [deletion](#deletion).

In an OVG, there are always two nodes: the `First` and the `Last`, which are present to indicate where the OVG starts and where it ends when traversing it.

Here are some examples of valid OVGs:
![Empty OVG](images/ovg/empty_ovg.png)
![Simple OVG](images/ovg/simple_ovg.png)
![Complex OVG](images/ovg/complex_ovg.png)


## Conflicts

The link to the the child node is stored using an [SVG](svg.md), which, as we will see, allows managing conflicts and their resolution.

A conflict in an OVG is represented by multiple possible paths to traverse from `First` to `Last`.

For example, here are some OVGs with conflicts:
![Simple conflict 1](images/ovg/simple_conflict_1.png)
![Simple conflict 2](images/ovg/simple_conflict_2.png)

An "empty" path is also considered a conflict. As soon as a link splits (the link's underlying [SVG](svg.md) contains more than one current value), we consider there to be a conflict.

For example, in the case below, we consider there to be a conflict regarding the existence of `A`:
![Conflict not solved](images/ovg/conflict_not_solved.png)

Conflict resolution is fairly simple: you just need to modify the links (using the [`Replace`](svg.md#operations) operation of the underlying [SVG](svg.md)) so that the conflict no longer exists.

For example, the previous conflict can be resolved by replacing the child link of `First` with a link to `A`:
![Conflict solved](images/ovg/conflict_solved.png)

As a note, for security reasons, direct modification of the underlying [SVG](svg.md) is usually not allowed. We will discuss in the section on [modification operations](#operations) how the graph can be modified.


## Deletion

When we want to delete a node, we cannot simply remove it from the graph because another user, unaware of the deletion, might potentially add links to this node before syncing.

Therefore, instead of deleting the node, we store an additional value in each node to indicate whether it has been deleted or not. We call this value the node's existence (we represent deleted nodes by coloring them in red in the examples).

For example, let's consider this OVG:
![Basic deletion base](images/ovg/basic_deletion_base.png)

If we want to delete node `B`, we must mark it as deleted and then change the child link of `A` so that it points to `Last`:
![Basic deletion](images/ovg/basic_deletion.png)

Unfortunately, this method does not work perfectly. Look at this OVG:
![Bad suppression and insertion conflict base](images/ovg/bad_deletion_and_insertion_conflict_base.png)

Imagine we decide to delete both `A` and `C`, but at the same time, another user decides to add a node `B` between `A` and `C`. After merging the two changes, we get this OVG:
![Bad suppression and insertion conflict](images/ovg/bad_deletion_and_insertion_conflict.png)

We end up with a graph with no conflicts, but adding a node in the middle of several deleted elements should result in a conflict.

To resolve this issue, we need to store the link to the previous node (also called the parent of the node, represented by the red arrows in the examples) in each node in addition to the link to the next node.

If we do this in our previous example, we get:
![Suppression and insertion conflict](images/ovg/deletion_and_insertion_conflict.png)

Now, we can tell if a conflict is present or not. To do this, we must start from all the nodes that are not deleted and traverse the graph to reconstruct all the missing links:
![Suppression and insertion conflict reconstruction](images/ovg/deletion_and_insertion_conflict_reconstruction.png)

Now, if we traverse the graph starting from `First`, we clearly see the conflict.

## Operations

To modify an OVG, we can simply change the links using the `Replace` [operation](svg.md#operations) of the underlying SVGs.

However, the problem with this method is that it allows the user to create cycles in the OVG (which would make it invalid and therefore unusable).

To avoid this problem, we will limit the possible operations to two "meta" operations, which can then be converted into simple `Replace` [operations](svg.md#operations) for the underlying SVGs.

### Insert
The `Insert` operation consists of inserting a list of nodes between two nodes, the `before` node and the `after` node. As the names suggest, the `before` node must be positioned before the `after` node in the OVG.

This operation can then be broken down into simpler steps:
- `Replace` the child of `before` with the first node to insert.
- Add the links between each node to be inserted by using the `Replace` operation on their parent and child.
- `Replace` the parent of `after` with the last node to be inserted.

Note: To generate the `Replace` operations from our `Insert` operation, it must contain the heads to replace in the links of the `before` and `after` nodes.

### Delete
The `Delete` operation consists of deleting a list of nodes between two nodes, the `before` node and the `after` node. As the names suggest, the `before` node must be positioned before the `after` node in the OVG.

For the operation to be valid, all nodes to be deleted must indeed be located between `before` and `after`.

This operation can then be broken down into simpler steps:
- `Replace` the child of `before` with the `after` node.
- Mark each deleted node as deleted in the OVG.
- `Replace` the parent of `after` with the `before` node.

Note: To generate the `Replace` operations from our `Delete` operation, it must contain the heads to replace in the links of the `before` and `after` nodes.

### Workaround

By using only these "meta" operations, it is not possible to create loops in the OVG because it is only possible to insert links between two ordered nodes.

However, it is no longer possible to resolve conflicts as we did previously. Fortunately, resolving a conflict is quite simple with these operations: you just need to delete the entire conflict using the `Delete` operation and then use the `Insert` operation to place the desired value at the conflict's location.
