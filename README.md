# sshtup - tuplespace server

sshtup is a tuplespace server that provides a shell interpreter interface for
the [Linda coordination language](https://en.wikipedia.org/wiki/Linda_(coordination_language)).
Normal SSH clients may connect to a sshtup server (`sshtupd`) to exchange data
and coordinate distributed operations in tuplespace.

# Example

```
alice$ ssh tuplesaurus.example.net
sshtupd: welcome to tuplespace

> in[1,2,3]
```

Alice's shell is now blocked waiting for the tuple [1,2,3] to appear in tuplespace.

```
bob$ ssh tuplesaurus.example.net
sshtupd: welcome to tuplespace

> out[1,2,3]
ok
```

This unblocks the `in` operation Alice's shell, which then displays the matching tuple:

```
Tuple([1,2,3])
> 
```

# TODO

A blocking Linda operation (`in`, `rd`) will unblock when a matching tuple is
written with `out` elsewhere. However, the thrussh server reactor doesn't
immediately pick up on this to display the result. In the example above, I kind
of cheated, Alice has to press enter to 'wake up' the tokio reactor.

sshtup currently stores tuples in memory. Durable, scalable storage would be nice.

All kinds of mundane things like identities, acls, permissions, timeouts,
policies, strong typing of tuples, etc. Ideally, these themselves would be
composed out of tuples and Linda coordination.

A proof-of-concept distributed system made of bash & ssh because why not.

