fn main() {
    // Embute o ícone e as informações de versão no .exe (somente ao compilar no Windows).
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Zek Download");
        res.set("FileDescription", "Zek Download - baixador de videos e musicas");
        res.set("LegalCopyright", "MIT License - aaaiiicerol");
        if let Err(e) = res.compile() {
            println!("cargo:warning=nao foi possivel embutir o icone: {}", e);
        }
    }
}
