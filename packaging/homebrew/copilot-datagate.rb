# Template Homebrew formula for DataGate.
# Update version, URLs, and sha256 values when publishing a release.

class CopilotDatagate < Formula
  desc "Policy-driven, read-only MCP server for safe database access from AI tools"
  homepage "https://github.com/afurlane/copilot-datagate"
  license "Apache-2.0"
  version "0.3.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-macos-aarch64.tar.gz"
      sha256 "TODO_REPLACE_WITH_MACOS_AARCH64_SHA256"
    else
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-macos-x86_64.tar.gz"
      sha256 "TODO_REPLACE_WITH_MACOS_X86_64_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-linux-aarch64.tar.gz"
      sha256 "TODO_REPLACE_WITH_LINUX_AARCH64_SHA256"
    else
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-linux-x86_64.tar.gz"
      sha256 "TODO_REPLACE_WITH_LINUX_X86_64_SHA256"
    end
  end

  def install
    bin.install "copilot-datagate"
  end

  test do
    assert_match "copilot-datagate", shell_output("#{bin}/copilot-datagate --help")
  end
end
