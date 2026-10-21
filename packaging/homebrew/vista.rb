# Homebrew formula template for a tap (e.g. github.com/<you>/homebrew-tap).
# After a tagged release, fill in the version, URLs and sha256 values from the
# release assets (`*.tar.gz.sha256`).
class Vista < Formula
  desc "Lean, IntelliJ-style terminal file browser with git, search and diffs"
  homepage "https://github.com/OWNER/vista"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/OWNER/vista/releases/download/v0.1.0/vista-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "FILL_ME"
    end
    on_intel do
      url "https://github.com/OWNER/vista/releases/download/v0.1.0/vista-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "FILL_ME"
    end
  end

  on_linux do
    url "https://github.com/OWNER/vista/releases/download/v0.1.0/vista-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "FILL_ME"
  end

  depends_on "git"

  def install
    bin.install "vista"
  end

  test do
    assert_match "vista #{version}", shell_output("#{bin}/vista --version")
  end
end
