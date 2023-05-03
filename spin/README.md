# Verification with the SPIN model checker

This directory contains a PROMELA implementation of Snowdon's lock-free snowflake generator implementation. You can
find it in [snowflake.pml](snowflake.pml). The file contains copious amounts of comments, so it should be understandable
even without knowing PROMOLA.

## Running

You can verify the model in `snowflake.pml` by installing SPIN and running the following commands:

```shell
spin -a snowflake.pml
# You can add `-O3 -flto` after `cc` to enable various compiler optimizations and LTO
cc -DSAFETY -DBFS_PAR -DNOCLAIM -w -o pan pan.c
./pan
```

This should check for `assert` violations and invalid end states. If you're familiar with SPIN, you can change the
compile time and runtime options to your liking, of course. For more information about SPIN and PROMELA, refer to SPIN's
official website <https://spinroot.com/>.

## Lock-free snowflake implementation

The snowflake generator we provide a model for in this directory is *lock-free*. I.e., regardless of how many threads
running the snowflake generator algorithm you suspend (at any point), other threads can't be suspended or fail as a
consequence. Moreover, at any given point, at least one thread in the system makes progress.

Note that this **doesn't mean** that our implementation is wait-free. Specifically, it's not guaranteed that a thread
generates a snowflake in a finite number of steps. While it's incredibly unlikely, other threads could consistently
interleave in a way that forces a thread to keep spinning while attempting to generate new IDs. However, the chance of
this happening is negligible even for systems that need to generate thousands of snowflakes per millisecond.

The general idea of this simple lock-free implementation is that we store the last generated snowflake as an atomic
integer in the generator's state. When generating a new snowflake, we simply load this snowflake, use it to create a new
snowflake (incrementing the counter if needed), and then store it again using a compare-and-swap operation. If this
operation fails, another thread has since generated another snowflake, so we try again.

With this implementation, it's guaranteed that there's progress in the system at any given point regardless of the
individual threads' behaviour. To make another thread retry the snowflake generation, at least one thread needs to
succeed.
