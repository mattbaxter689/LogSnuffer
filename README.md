# LogSnuffer

The reasoning for this project actually originated from something related to my regular day job.
There were some issues related to the monitoring services used, and I began wondering if an agentic approach was used instead for this sort of thing. IE: compute metrics and confidence scores from logs of various systems, and use that to assess things via LLM call for reasoning and possible alerting. For this project, we'll focus specifically on capturing error logs, but it can easily be updated to include any type of log data, and classify them accordingly. For example, you can have some form of log summarizer attached to this, you can have internal log errors, service errors for services that create tickets if a customer has issues. There are many applications for this sort of piece, with companies having numerous services and capturing everything, this can be a big issue with processing times.

## Methodology

minikube addons enable ingress
minikube image load image:tag
kubectl get pods
helm upgrade --install snuff ./snuff -f snuff/values-env.yaml
 helm upgrade snuff ./snuff --set replicaCount=0
echo "$(minikube ip) snuff.local" | sudo tee -a /etc/hosts
