fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon_with_id("", "32512");
        res.set("FileDescription", "Invoice Payables — AP Review Tool");
        res.set("ProductName", "Invoice Payables");
        res.set("OriginalFilename", "invoice-payables.exe");
        if let Err(e) = res.compile() {
            eprintln!("winres compile error: {}", e);
        }
    }
}
