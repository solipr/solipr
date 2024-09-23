# Single Value Graph

A **Single Value Graph** (SVG) is a data structure used to store a single value and enable you to **manage conflicts**. It is the main data structure used in **Solipr**.

For example, if you want to store the name of a file so that multiple people can modify it in parallel and manage conflicts correctly, you would use an SVG:

- Francis names the file "READMA.txt"
- David sees the error and corrects the file name to "README.txt"

Here, there is no conflict. But if at the same time:
- Guillaume decides to change the file name to "README.md"

There will be a conflict between him (README.md) and David (README.txt) because they both modified the file "READMA.txt".

```mermaid
---
title: Example Illustration
---
graph LR
    subgraph "Conflict"
        B(David: README.txt)
        C(Guillaume: README.md)
    end

    A(Francis: REAMDA.txt) --> B
    A --> C
```

## Storage

SVGs are stored in the form of a [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph) where the current value is represented by nodes not pointed to by any other node (represented with a red outline in the examples). Throughout this documentation, we will call these nodes the heads.

Here are examples of SVGs and their current values:
```mermaid
graph
    classDef head stroke:#f00

    subgraph "Current Value = A and C"
        direction BT
        A4(A):::head
        C4(C):::head --> B4(B)
    end

    subgraph "Current Value = D"
        direction BT
        C3(C) --> A3(A)
        B3(B) --> A3(A)
        D3(D):::head --> C3(C)
        D3(D) --> B3(B)
    end

    subgraph "Current Value = B and C"
        direction BT
        C2(C):::head --> A2(A)
        B2(B):::head --> A2(A)
    end

    subgraph "Current Value = C"
        direction BT
        C1(C):::head --> B1(B)
        B1(B) --> A1(A)
    end
```

## Conflicts

An SVG is considered to represent a conflict if it contains more than one head. For example, in the previous example, the second and fourth SVGs are considered to represent a conflict.

Resolving a conflict is very simple, just add a new node pointing to each head of the conflict:

```mermaid
graph
    classDef head stroke:#f00

    subgraph "After (no conflict)"
        direction BT
        C3(C) --> A3(A)
        B3(B) --> A3(A)
        D3(D):::head --> C3(C)
        D3(D) --> B3(B)
    end

    subgraph "Before (conflict)"
        direction BT
        C2(C):::head --> A2(A)
        B2(B):::head --> A2(A)
    end
```

## Operations

There is only one possible operation in an SVG: `Replace`, this operation consists of replacing one or more nodes with a new one. This operation simply adds a new node in the SVG pointing to the nodes to be replaced.

```mermaid
---
title: Multiple operations on a SVG step by step (without conflicts)
---
graph
    classDef head stroke:#f00

    subgraph "Replace([B], C)"
        direction BT
        C3(C):::head --> B3(B)
        B3(B) --> A3(A)
    end
    
    subgraph "Replace([A], B)"
        direction BT
        B2(B):::head --> A2(A)
    end

    subgraph "Replace([], A)"
        direction BT
        A1(A):::head
    end

    subgraph "Nothing"
    end
```

```mermaid
---
title: Multiple operations on a SVG step by step (with conflict and resolution)
---
graph
    classDef head stroke:#f00

    subgraph "Replace([C, B], D)"
        direction BT
        D4(D):::head --> C4(C)
        D4(D) --> B4(A)
        C4(C) --> A4(A)
        B4(B) --> A4(A)
    end

    subgraph "Replace([A], C)"
        direction BT
        C3(C):::head --> A3(A)
        B3(B):::head --> A3(A)
    end

    subgraph "Replace([A], B)"
        direction BT
        B2(B):::head --> A2(A)
    end
    
    subgraph "Replace([], A)"
        direction BT
        A1(A):::head
    end
    
    subgraph "Nothing"
    end
```

## Apply Order

Applying the same operations in a different order always leads to the same state, making our SVG a [CRDT](https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type).
