{{- define "ares.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/* Release "ares" yields "ares" (not "ares-ares"); other releases prefix. */}}
{{- define "ares.fullname" -}}
{{- if contains (include "ares.name" .) .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name (include "ares.name" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "ares.labels" -}}
app.kubernetes.io/part-of: {{ include "ares.name" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/* container image for a given service key */}}
{{- define "ares.image" -}}
{{- printf "%s/ares-%s:%s" .root.Values.global.imageRegistry .name (.svc.imageTag | default .root.Values.global.imageTag) -}}
{{- end -}}
