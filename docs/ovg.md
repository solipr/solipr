
# Ordered Value Graph

Les [SVG](svg.md) sont utile pour gere une seule valeur, mais le probleme est qu'ils ne permettent pas de gerer plusieurs valeurs ordonnes.

Pour cela nous avons cree une nouvelle structure de donnees que nous avons appele Ordered Value Graph (OVG).

Cette structure de donne est un [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph) dont chaque noeud correspond a une valeur.

Chaque noeud contient 4 informations, voici les deux premieres:
- La valeur contenu par le noeud (ici represente par le texte dans le noeud)
- Une liaison vers le noeud suivant (aussi appele la liaison enfante, represente par les fleches vert partant du noeud)

Nous verrons les deux autres dans la partie sur la [suppression](#deletion).

Dans un OVG, il y a toujours deux noeuds, le premier et le dernier qui sont present afin de savoir dans quel sens parcourir le graphe.

Voici quelques exemple d'OVG valides:
![Empty OVG](images/ovg/empty_ovg.png)
![Simple OVG](images/ovg/simple_ovg.png)
![Complex OVG](images/ovg/complex_ovg.png)


## Conflicts

Les liaisons vers le noeud parent ou le noeud enfant sont stocke a l'aide d'un [SVG](svg.md), ce qui, nous le verront, permet de gerer les conflits et leur resolution.

Un conflit dans un OVG est represente par plusieurs chemins possibles a emprinter pour aller de `First` vers `Last`.

Par exemple des exemples de OVG aillant des conflits:
![Simple conflict 1](images/ovg/simple_conflict_1.png)
![Simple conflict 2](images/ovg/simple_conflict_2.png)

Un chemin vide est aussi compte comme un conflit, a partir du moment ou un liaison se dedouble (le [SVG](svg.md) de la liaison contient plus d'une seule valeur courrante) on considere qu'il y a un conflit.

Par exemple dans l'exemple ci-dessus, on considere qu'il y a un conflit entre l'existance ou non de `A`:
![Conflict not solved](images/ovg/conflict_not_solved.png)

La resolution des conflits est plutot simple, il suffit de moddifier les liaisons (en utilisant l'operation [`Replace`](svg.md#operations) du [SVG](svg.md) sous jacent) de maniere a ce qu'il n'y ait plus de conflit.

Par exemple, l'exemple precedent peut etre resolu en replacant la liaison enfante de `First` par une liaison vers `A`:
![Conflict solved](images/ovg/conflict_solved.png)

Petite precision, pour une question de securite, la moddification directe du [SVG](svg.md) sous jacent n'est normalement pas autorisee. Nous verrons dans la partie sur [les operations de modification](#operations) comment on peut modifier le graphe.


## Deletion

Lorsque nous voulons supprimer un noeud, nous ne pouvons pas simplement le supprimer du graphe car un autre utilisateur n'ailant pas la connaisance de cette suppression peut pottentiellent ajouter des liaisons vers ce noeud dans le futur.

C'est donc pour cela qu'au lieu de le supprimer, nous stockons une valeur supplementaire dans chaques noeuds afin de savoir si il a ete supprime ou non, nous appelons cette valeur l'existance du noeud, nous representerons les noeuds supprime en mettant leurs contour en rouge.

Par exemple prenont cet OVG:
![Basic deletion base](images/ovg/basic_deletion_base.png)

Si nous voulons supprimer le noeud `B`, nous devons le marque comme etant supprime, puis nous devons changer la liaison enfante de `A` pour qu'elle pointe vers `Last`:
![Basic deletion](images/ovg/basic_deletion.png)

Malheureusement, cette methode ne fonctionne pas totalement, regardez ce graphe:
![Bad suppression and insertion conflict base](images/ovg/bad_deletion_and_insertion_conflict_base.png)

Imaginons que nous dessiodons de supprimer `A` et `C` mais qu'au meme moment, un autre utilisateur decide d'ajouter un noeud `B` entre `A` et `C`, apres avoir assembler les deux changements, nous aurons ce OVG:
![Bad suppression and insertion conflict](images/ovg/bad_deletion_and_insertion_conflict.png)

Nous finnissons avec un graphe sans aucuns conflit, or ajouter un noeud au milieu de plusieurs elements supprime devrait etre un conflit.

Afin de resoudre ce probleme, nous devons stocker la liaison vers le noeud precedent (aussi apelle la liaison parente, represente par les fleches rouge) dans chaque noeud en plus de la liaison vers le noeud suivant.

Si nous faisons cela dans notre exemple precedent, nous aurons:
![Suppression and insertion conflict](images/ovg/deletion_and_insertion_conflict.png)

Maintenant nous pouvons savoir si un conflit est present ou non, pour cela, nous devons partir de tous les noeuds qui ne sont pas supprime et parcourir le graph afin de reconstruire toutes les liaisons manquantes:
![Suppression and insertion conflict reconstruction](images/ovg/deletion_and_insertion_conflict_reconstruction.png)

Maintenant nous pouvons parcourir le graphe en partant de `First` et nous voyons bien le conflit.

## Operations

### Insert
### Delete
