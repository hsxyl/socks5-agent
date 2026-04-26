use yamux::Connection; fn check(c: Connection<()>) { yamux::into_stream(c); }
