import SwiftUI

struct RenderPanelView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Text("Offline Render")
        .font(.headline)

      HStack(spacing: 16) {
        Picker("Render Quality", selection: $state.renderQuality) {
          ForEach(RenderQualityOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.segmented)

        Picker("Output Format", selection: $state.exportFormat) {
          ForEach(ExportFormatOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 180)
      }

      HStack(spacing: 16) {
        Picker("ProRes FPS", selection: $state.proResFrameRate) {
          ForEach(ProResFrameRateOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 140)

        Picker("ProRes Profile", selection: $state.proResProfile) {
          ForEach(ProResExportProfile.allCases) { profile in
            Text(profile.displayName).tag(profile)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 260)
      }

      Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 8) {
        GridRow {
          Label("Project", systemImage: "doc.text")
          Text(state.projectPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Schema", systemImage: "checkmark.seal")
          Text(state.projectSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Analysis Cache", systemImage: "externaldrive")
          Text("No cache entries yet")
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Render Queue", systemImage: "list.bullet.rectangle")
          Text(state.renderQueueSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Preview Frame", systemImage: "rectangle.on.rectangle")
          Text(state.previewProbeSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("ProRes Export", systemImage: "film.stack")
          Text(state.proResPlanSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("ProRes Output", systemImage: "film")
          Text(state.proResExportSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Source A Frames", systemImage: "a.square")
          Text(state.frameSequenceModulatorPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Source B Frames", systemImage: "b.square")
          Text(state.frameSequenceCarrierPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Sequence Output", systemImage: "rectangle.stack.badge.play")
          Text(state.frameSequenceOutputPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Two-Source Render", systemImage: "point.3.connected.trianglepath.dotted")
          Text(state.frameSequenceSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
      }

      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Button {
            state.createTestProject()
          } label: {
            Label("Create Test Project", systemImage: "doc.badge.plus")
          }

          Button {
            state.openProject()
          } label: {
            Label("Open Project", systemImage: "folder.badge.gearshape")
          }

          Button {
            state.probeSelectedSources()
          } label: {
            Label("Probe Sources", systemImage: "waveform.path.ecg.rectangle")
          }

          Button {
            state.probePreviewFrames()
          } label: {
            Label("Probe Preview Frames", systemImage: "rectangle.on.rectangle")
          }
        }

        HStack {
          Button {
            state.runCpuReferenceRender()
          } label: {
            Label("Run CPU Reference Render", systemImage: "play.circle")
          }

          Button {
            state.runQueuedTestRender()
          } label: {
            Label("Run Queue Test", systemImage: "list.bullet.rectangle")
          }
        }

        HStack {
          Button {
            state.chooseFrameSequenceModulatorDirectory()
          } label: {
            Label("Source A Frames", systemImage: "a.square")
          }

          Button {
            state.chooseFrameSequenceCarrierDirectory()
          } label: {
            Label("Source B Frames", systemImage: "b.square")
          }

          Button {
            state.chooseFrameSequenceOutputDirectory()
          } label: {
            Label("Sequence Output", systemImage: "folder.badge.plus")
          }
        }

        HStack(spacing: 16) {
          Stepper(value: $state.frameSequenceAmount, in: 0...64, step: 1) {
            Text("Amount \(state.frameSequenceAmount, specifier: "%.0f")")
          }
          .frame(width: 140, alignment: .leading)

          Stepper(value: $state.frameSequenceMaxFrames, in: 1...600, step: 1) {
            Text("Max Frames \(state.frameSequenceMaxFrames)")
          }
          .frame(width: 170, alignment: .leading)

          Toggle("Flow Cache", isOn: $state.frameSequenceWritesFlowCache)
            .toggleStyle(.checkbox)
            .frame(width: 120, alignment: .leading)
        }

        HStack {
          Button {
            state.runTwoSourceFrameSequenceRender()
          } label: {
            Label("Run Two-Source Sequence", systemImage: "play.rectangle.on.rectangle")
          }

          Button {
            state.exportLastFrameSequenceProResMovie()
          } label: {
            Label("Export Sequence ProRes MOV", systemImage: "film.badge.plus")
          }
        }

        HStack {
          Button {
            state.checkProResExportPlan()
          } label: {
            Label("Check ProRes", systemImage: "film")
          }

          Button {
            state.exportRenderQueueProResMovie()
          } label: {
            Label("Export Queue ProRes MOV", systemImage: "film.badge.plus")
          }

          Button {
            state.exportProResMovie()
          } label: {
            Label("Export Frame Directory MOV", systemImage: "folder.badge.plus")
          }
        }
      }

      Text(state.statusMessage)
        .font(.caption)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
  }
}
