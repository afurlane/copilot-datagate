# Template Homebrew formula for DataGate 0.4.0.

class CopilotDatagate < Formula
  desc "Policy-driven, read-only MCP server for safe database access from AI tools"
  homepage "https://github.com/afurlane/copilot-datagate"
  license "Apache-2.0"
  version "0.4.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-macos-aarch64.tar.gz"
      sha256 "d03a7ce1ee181545f3123c73f29cf002c1c7f2cfd344b0c98ceccc4cc105e2ec"
    else
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-macos-x86_64.tar.gz"
      sha256 "d683b1c0cf407cebd26b5bda96a9c7cd017d6ef217810e24b0719fbc32683a0c"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-linux-aarch64.tar.gz"
      sha256 "6407a08448fdef383eb2781fb3b9b57ab7c971fb55390c5ff69172949118fc1c"
    else
      url "https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v#{version}/copilot-datagate-copilot-datagate-v#{version}-linux-x86_64.tar.gz"
      sha256 "a5603095d10a7ac46afc3cb30d64ee16d1005f06fcd59ceee0e94f2778f5c483"
    end
  end

  def install
    bin.install "copilot-datagate"
  end

  test do
    assert_match "copilot-datagate", shell_output("#{bin}/copilot-datagate --help")
  end
end
