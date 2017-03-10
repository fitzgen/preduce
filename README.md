**⚠ ⚠ ⚠**

**This is very much an incomplete and work-in-progress prototype!! Any claims
made below are likely false.**

**⚠ ⚠ ⚠**

# `preduce`

A parallel, language-agnostic, automatic test case reducer.

`preduce` takes a large test case that exhibits some interesting behavior (such
as crashing your program) and finds a smaller test case that exhibits the same
interesting behavior.

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->


- [What? Why?](#what-why)
- [Installing `preduce`](#installing-preduce)
- [Using `preduce`](#using-preduce)
  - [Writing an Is-Interesting? Predicate Script](#writing-an-is-interesting-predicate-script)
  - [Using `preduce` as a Libary](#using-preduce-as-a-libary)
- [How `preduce` Works](#how-preduce-works)
  - [Reducers](#reducers)
  - [Is-Interesting? Predicates](#is-interesting-predicates)
  - [Traversal of the Search Space](#traversal-of-the-search-space)
    - [Reframing the Problem](#reframing-the-problem)
- [Why Not Merge `preduce` with C-Reduce?](#why-not-merge-preduce-with-c-reduce)
- [Areas of Future Exploration](#areas-of-future-exploration)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

## What? Why?

Minimal test cases are the most useful: they contain only the bits necessary to
trigger the bug. Everything else is actively detrimental to understanding and
fixing the bug because it must be tediously considered and disregarded by the
programmer, wasting her valuable time.

At the same time, creating minimal test cases is often difficult. Requiring bug
reporters to provide a minimal test case results in valuable bugs not getting
reported. Even for a program's developers, who are intimately familiar with its
inner workings, isolating the relevant bits of a huge test case can be like
finding a needle in a haystack.

So why not automate the test case reduction process? This is exactly what
`preduce`, and programs that inspired it, provide.

For more background:

* [Delta Debugging on Wikipedia](https://en.wikipedia.org/wiki/Delta_Debugging)

* [C-Reduce](http://embed.cs.utah.edu/creduce/), an *excellent* automatic test
  case reducer geared mostly towards C and C++ test cases, but also usable with
  other curly brace languages. `preduce` is directly inspired by C-Reduce, but
  has the goal of better utilizing many-core machines to reduce test cases
  faster. For more details, see the implementation notes further below.

## Installing `preduce`

First, [install Rust and its package manager `cargo`](https://rustup.rs/) if you
don't already have them installed.

Then, use `cargo` to install `preduce`:

```
$ cargo install preduce
```

`preduce` should now be installed at `$HOME/.cargo/bin/preduce`, so make sure
that `$HOME/.cargo/bin` is on your `$PATH`.

## Using `preduce`

To use `preduce`, invoke it with the script that implements the "is this test
case interesting?" predicate and the test case.

```
$ preduce [options] ./path/to/predicate ./path/to/test/case
```

For information about the various options available, run

```
$ preduce --help
```

### Writing an Is-Interesting? Predicate Script

Predicate scripts are invoked with a single argument: a relative path the
potentially-interesting test case file it should judge. The predicate script
must communicate whether the test case is interesting or not by exiting `0` for
interesting, or non-zero for uninteresting.

The predicate script **must** be deterministic. If it isn't, `preduce`'s results
won't be useful, and they won't likely be very reduced either. Garbage in,
garbage out.

Here are some tips for shell scripts:

* Start with `set -eu` to exit non-zero if any subcommand fails, or an undefined
  variable is used.
* Leverage `grep`'s non-zero exit when it doesn't find any matches. This can be
  used to hunt for a particular error message from your program, or early exit
  when you know the grepped-for pattern must appear in the test case input file
  to trigger your bug.

### Using `preduce` as a Libary

For programmatic control over reduction strategies and is-interesting
predicates, you can use the `preduce` crate as a library.

Create a new executable crate with `cargo`:

```
$ cargo new --bin my-custom-test-case-reducer
$ cd my-custom-test-case-reducer/
```

Then, add `preduce` as a dependency in your `Cargo.toml`:

```toml
[dependencies]
preduce = "<insert latest preduce version here>"
```

For library usage documentation, see
the [`preduce` library documentation on docs.rs](https://docs.rs/preduce).

## How `preduce` Works

`preduce` is comprised of three parts:

1. A test case reducer, which generates smaller test cases of unknown
   interesting-ness from an initial, larger, known-interesting test case.

2. A predicate to test whether a given test case is interesting or not.

3. Traversal of the reduced test case search space, which leverages (1) and (2).

### Reducers

A test case reducer is given a known-interesting test case, and yields a series
of potentially interesting reductions of the known-interesting test case. Test
case reducers are typically implemented as scripts invoked as subprocesses of
`preduce`. This design is mimicked from C-Reduce, and allows `preduce` to reuse
all of C-Reduce's test case reducers.

`preduce`'s builtin test case reduction strategies include:

* Removing a line of text
* Removing a comment
* Removing a contiguous, indented chunk of text
* And all of C-Reduce's other reduction strategies

Users may also provide their own reducers to be used alongside or instead of the
builtin set of reducers.

Custom reducers **must** produce *reductions* of their seed test case: each new
test case must be smaller in size than the seed. If this property is not held,
`preduce` is not guaranteed to terminate.

### Is-Interesting? Predicates

The "is this test case interesting?" predicates are wholly supplied by the user,
and are opaque to `preduce`. `preduce` simply invokes them. Additionally, it is
`preduce` that manages all concurrency and parallelism, and both the predicate
scripts and the reducers need not be privy.

Once again, this mimics C-Reduce's design.

### Traversal of the Search Space

To best describe `preduce`'s search strategy is to contrast it with C-Reduce's
search strategy, and describe the motivations that led me to implementing an
alternative. What follows is largely an adaptation and summarization of John
Regehr's blog post about making C-Reduce parallel in the first
place: [Parallelizing Delta Debugging](http://blog.regehr.org/archives/749).

Empirically, most reductions are not interesting. C-Reduce is most often used
with C/C++ compilers: whether they crash when compiling the test case, whether
they miscompile it, etc. Indeed, I was using C-Reduce for a compiler as well,
just not a typical one: `rust-bindgen`, which takes C/C++ headers and emits
extern FFI declaration boiler plate for Rust programs that want to use the C/C++
header's library. In such a scenario, it is easy to see that most reductions
will change the semantics of the C/C++ program in ways such that the buggy code
path in the compiler is not triggered, or, even more likely, produce an invalid
C/C++ program.

With that in mind, let's discuss C-Reduce's approach to parallelizing delta
debugging.

C-Reduce has an initial, known-interesting test case, and spawns off
N=number-of-cores workers to test each one's interesting-ness.

          version O
             /|\
            / | \
           /  |  \
          /   |   \
         ?    ?    ?

Two of the workers complete, and find their test cases uninteresting, so
C-Reduce spawns two new workers with the next reductions to be judged for
interesting-ness.

        version O ----.
           /|\      \  \
          / | \      \  \
         /  |  \      \  \
        /   |   \      \  \
       X    X    ?      ?  ?

Then a worker finds its test case interesting. All active workers' test cases
are abandoned, and C-Reduce begins generating and testing reductions of the new,
smaller test case.

         version O -------.
           /|  \        \  \
          / |   \        \  \
         /  |    \        \  \
        /   |     \        \  \
       X    X   version 1   X  X
                   /|\
                  / | \
                 /  |  \
                /   |   \
               ?    ?    ?

This greedy search is repeated to a fixpoint where no further reductions are
interesting.

C-Reduce doesn't worry about the work that is abandoned whenever a new reduction
is judged interesting because most reductions aren't interesting.

But many reductions are independent of each other, for example removing one
function is often independent from removing another function. We can generate
the "same" reduction once again for the new, reduced test case: same
function-removal diff, different test case to apply the diff to. And then we
begin testing that "same" reduction again, only to lose the race to be the first
worker to confirm interesting-ness once again, and we repeat this race-losing
process potentially many times. The more workers we have, and the more
parallelization we introduce, the more likely each worker is to lose the race,
and the more likely we are to repeat the same work over and over.

John Regehr found that performance with four workers was an improvement over one
worker, but that five workers was worse than one, and as even more are added
performance falls off a cliff. On the machine I'm working with, I have 48
logical cores, and I **really** want to utilize them, because C-Reduce runs
often take a day and half when minimizing test cases for `rust-bindgen`.

John Regehr describes in his blog post how he initially investigated each core
having its own copy of the smallest interesting test case found so far, and
periodically merging them. He ran into these problems:

* The `merge` tool is very conservative, and will often fail with conflicts.
* He was unsure how often to merge.

Like C-Reduce, `preduce` maintains a global most-reduced known-interesting test
case. Unlike C-Reduce, `preduce` does not abandon the work that workers are
performing when a new most-reduced interesting test case is discovered, and
instead let's them finish. If their test case is smaller than the current
most-reduced interesting test case, then it becomes the new most-reduced
interesting test case, and we start searching reductions generated from this new
test case. `preduce` also generates a merge between the new test case and what
was (or still is, if this test case is smaller) the most-reduced interesting
test case. This is inserted into the queue of reductions to test for
interesting-ness the same as any other generated reduction. If the merge fails,
then it is discarded.

This approach provides a definite answer to how often to merge: whenever an
interesting test case is found. As for conflicts and merge failures, `preduce`
builds a `git` history of the test case's reductions and uses `git`'s awareness
of history to make more merges succeed. It also does dumb little tricks like
shuffling the order of generated reductions so that a reducer which generates
reductions in order (eg remove the first line, remove the second line, ...)
doesn't accidentally make every reduction conflict with each other.

Here is an illustration of `preduce`'s merging approach. A reduction is found
interesting while two more are still being judged. This spawns off new workers
for new reduction of the new test case. Some of these are judged uninteresting
quickly:

         version O ------------.
           /|  \          \     \
          / |   \          \     \
         /  |    \          \     \
        /   |     \          \     \
       X    X   version 1.0   ?     ?
                   /|\
                  / | \
                 /  |  \
                /   |   \
               ?    X    X

Meanwhile, one of the reductions of the initial test case is also judged
interesting. This kicks off a new worker, attempting to do a merge between the
interesting reductions:

         version O ------------.
           /|  \          \     \
          / |   \          \     \
         /  |    \          \     \
        /   |     \          \     \
       X    X   version 1.0   ?  version 1.1
                   /|\     \       /
                  / | \     \     /
                 /  |  \     \   /
                /   |   \     \ /
               ?    X    X     ?

Next, the last direct reduction of version 0 is judged uninteresting. We start
testing the next reduction of the current most-reduced interesting test
case. For illustrative purposes, let's say that is version 1.0 (it could be
either 1.0 or 1.1, it makes no difference).

         version O ------------.
           /|  \          \     \
          / |   \          \     \
         /  |    \          \     \
        /   |     \          \     \
       X    X   version 1.0   X  version 1.1
                /  /|\     \       /
               /  / | \     \     /
              /  /  |  \     \   /
             /  /   |   \     \ /
            ?  ?    X    X     ?

The merge between 1.0 and 1.1 is found interesting, and becomes the smallest .

         version O ------------.
           /|  \          \     \
          / |   \          \     \
         /  |    \          \     \
        /   |     \          \     \
       X    X   version 1.0   X  version 1.1
                /  /|\     \       /
               /  / | \     \     /
              /  /  |  \     \   /
             /  /   |   \     \ /
            ?  ?    X    X  version 2.0
                                \
                                 \
                                  \
                                   \
                                    ?

This process continues until we reach a fixed point where there are no more
reductions to make, or all reductions are found to be uninteresting.

Like C-Reduce's approach, this merge approach is also greedy. Just "slightly
less" greedy, in that we try to avoid throwing away work we've aready started to
do with the merges. Fundamentally, at each step we still only consider
reductions of the current best choice.

In order for this merging approach to be an improvement over C-Reduce's greedy,
work-abandoning approach, the following assumptions must hold:

* Merging two interesting reduced test cases should usually result in another
  interesting test case.

* The cost of letting all is-interesting tests run to completion does not
  outweigh the benefit we get from merging. This is usually the case because
  uninteresting cases are usually judged quicker, so any tests that are still
  running when the first completes are more likely to be interesting as well.

#### Reframing the Problem

> How do we know `preduce` will reach a fixed point and terminate? This just
> sounds like a minor heuristics-based tweak to C-Reduce's searching, isn't
> there something with a big-O improvement? I'm not satisfied!

Patience. I'm not satisfied either! But I don't have anything better yet.

Reframing the problem might help get the creative juices going, so here is a
little brain dump. Lots of handwaving, simplifications, and assumptions
incoming.

We are given:

* An is-interesting predicate function *f* mapping test cases to boolean.

* An initial test case *T*.

* The finite set of all the reductions we can make, *R = { r<sub>0</sub>,
  r<sub>1</sub>, ..., r<sub>n</sub> }*. Applying a set of reductions *r ⊆ R* to
  *T* produces the reduced test case *T<sub>r</sub>*. That is, *r* are patch
  operations producing reduced test cases, not reduced test cases themselves.
  For simplicity, we assume that either reductions are associative and
  commutative, or can be sorted meaningfully before applying them to *T*.

With this in hand, we define the search space as the lattice produced by the
powerset of *R*, with a partial ordering defined by set inclusion. There are
2<sup>*n*</sup> elements in the search space.

We begin our search from the bottom element: the empty set of reductions, ie the
initial test case. We are ostensibly searching for the maximal reduction set
*r<sub>max</sub>* for which *f* returns true: *f(r<sub>max</sub>(T)) =
true*. However, searching 2<sup>*n*</sup> elements is much too many to be
practical, so we are searching for a *maximal enough* reduction set.

C-Reduce makes a greedy depth first search up the lattice. It tests each
reduction set *|r| = 1* until *f* returns true, and then tests each reduction
set *|r| = 2* reachable by adding a single new reduction to the last reduction
set *|r| = 1* for which *f* returned true, etc... until it reaches a fixed
point. It only adds reductions to the reduction set at any given step, and so
the process is monotone. The fixed point is reached either because it walks to
the top of the lattice (all reductions applied, the empty test case) or *f*
returns false for all reduction sets with one more item than the current
reduction set. There are *n* reduction sets *|r| = 1*, *n - 1* reduction sets
*|r| = 2*, ..., 1 reduction set *|r| = n*. Therefore, it has running time of
*O(n<sup>2</sup>)*.

Merging fits right in with this reframing of the problem: it is the lattice's
join operation: union of reduction sets. This preserves the monotone property of
the process. It also runs in *O(n<sup>2</sup>)*.

C-Reduce will go up the first path it finds where *f* is returning true, and may
miss paths that ultimately go further up the lattice to a more larger reduction
set, but begin "after" the entry point to the current reduction set. The merging
approach will find these paths *if* it began testing their entries for
interesting-ness before the current path's entry was found interesting. The more
parallel workers testing interesting-ness, the more likely to find such paths'
entry points. This means that more parallel workers are a boon in a merging
traversal, while in C-Reduce's current approach they are more likely to lose the
race to first-interesting-reduction and do useless work. This is a nice
improvement, if nothing else.

Yet another alternative is to perform a depth-first search over the entire
lattice space, but formalize the *maximal enough* property into a function of
running time or size of the reduced test case. Then, if we ever find a reduction
set that is both interesting and satisfies the *maximal enough* function, we
stop.

Maybe there is some more clever property of the lattice we can exploit to
discover a *maximal enough* reduction set in much faster time, or with much
better parallelism. If you have ideas, please let me know!

## Why Not Merge `preduce` with C-Reduce?

> Since `preduce` already shares much of its design with C-Reduce, and only
> improves on the orchestration of the traversal of the search space, why not
> merge it with C-Reduce?

Yes! That would be lovely!

`preduce` exists as a prototype to better parallelize C-Reduce, and (assuming
that it meets that goal) it would be great to either replace C-Reduce's search
orchestration with `preduce`'s or port the algorithm to C-Reduce.

## Areas of Future Exploration

Allow `preduce` to distribute work across many different machines, not just
cores on one machine.
