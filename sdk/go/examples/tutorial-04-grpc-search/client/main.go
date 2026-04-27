// Demo client for tutorial 04. Sends a list of URLs and prints
// pages as the worker pool finishes them.
//
//	go run . [-workers 8] url1 url2 url3 ...
package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"log"

	pb "github.com/offbit-ai/reflow/sdk/go/examples/tutorial-04-grpc-search/proto"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func main() {
	addr := flag.String("addr", "localhost:50404", "server address")
	workers := flag.Uint("workers", 0, "0 = server default")
	flag.Parse()
	urls := flag.Args()
	if len(urls) == 0 {
		urls = []string{
			"https://en.wikipedia.org/wiki/Flow-based_programming",
			"https://en.wikipedia.org/wiki/Actor_model",
			"https://en.wikipedia.org/wiki/Dataflow_programming",
			"https://en.wikipedia.org/wiki/Reactive_programming",
			"https://en.wikipedia.org/wiki/Communicating_sequential_processes",
		}
	}

	conn, err := grpc.Dial(*addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Fatalf("dial: %v", err)
	}
	defer conn.Close()

	client := pb.NewCrawlerClient(conn)
	stream, err := client.Crawl(context.Background(), &pb.CrawlRequest{
		Urls: urls, Workers: uint32(*workers),
	})
	if err != nil {
		log.Fatalf("crawl: %v", err)
	}

	for {
		page, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			log.Fatalf("recv: %v", err)
		}
		fmt.Printf("%4dms  %3d  %-90s  %s\n",
			page.TookMs, page.Status, truncate(page.Title, 90), page.Url)
	}
}

func truncate(s string, n int) string {
	if len(s) > n {
		return s[:n-1] + "…"
	}
	return s
}
