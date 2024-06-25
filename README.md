# Folder Transfer

This is my first project, written in the Rust programming language.
It is a console application for streaming files between computers in the same LAN.
I'm heavily using the [tokio crate](https://crates.io/crates/tokio) for handling asynchronous tasks and networking and also the [ratatui crate](https://crates.io/crates/ratatui) for displaying some minimalistic UI inside the terminal.

Here is what the app does, in general:
   1) Sends UDP broadcast requests on a specific network interface (chosen by the user), in attempt to find any other running peer apps
   2) Connect to a peer
   3) Streams all the contents of a particular folder, from one peer to another

Streaming of a single file is done using a couple of concurrent TCP connections, which allows utilizing a big portion of the network bandwidth.
Over a standard 1 Gbps connection in the local area netwotk, the file streaming speed reaches roughly 110 MB/s.

If you want to check the app out, you can compile it from source, provided that you have Rust and Cargo installed on your system.
Having them set up, building the project is as simple as running 'cargo build' from the terminal.
