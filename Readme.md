# xv6-sh transpiled to Rust and converted to async

Once you get over the fact that the code makes your eyes bleed, it has a sort of beauty.

The goal of this was not to use the resulting shell (it's a toy), but as a pathfinder for
a similiar translation of the much larger dash shell. Many thanks to Andy Chu who 
[suggested this](https://lobste.rs/s/bl7sla/what_are_you_doing_this_weekend#c_spxrrg) as
a reasonable experiment to run before sinking in too much time.

Experiments are the best time to use new technology, and this one is no different. As a
result of using O_NONBLOCK pidfd's in spawn.rs to reap children **this requires linux 5.10 
or later**. Note that at the time of writing the most recently published version of linux 
is 5.10-rc6, which is also the version this was tested on. This feels like the future,
but if you're looking to use this code for anything non-toy in the nearterm I would suggest
using a thread to wait on the pidfd's instead. You might want to take advantage of pop-os's
[pidfd](https://github.com/pop-os/pidfd/) library if you're writing in rust.

From a personal perspective this was a very worthwhile pathfinder, things that I'm glad I
took away from it include

 - Needing to be careful what references are captured in async blocks (e.g. `&self`)
 - How to reap children in async code
 - That handling control flow of the form `fork(); if (pid == 0 and we reach an error) { exit() }`
   is somewaht painful and requires bubbling up errors long distances. Potentially we could use
   and catch `panic` for this, but I'd rather not.
 - I need to put some thought into the ownership of file descriptors, because we need to free them
   at the right times (e.g. with pipes...). 

Run with `cargo run`.

See `c2rust.md` for brief notes on the commands used to translate `sh.c` with `c2rust`. I've also
massaged the commit history of this repo a bit, so it may be instructive to look at the commits.